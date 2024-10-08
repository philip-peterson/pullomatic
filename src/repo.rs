use config::{Config, Credentials};
use git2;
use std::error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;


#[derive(Debug)]
struct RepoState {
    last_checked: Option<Instant>,
    last_changed: Option<Instant>,
}

#[derive(Debug)]
pub struct Repo {
    name: String,
    config: Config,

    state: Mutex<RepoState>,
}

#[derive(Debug)]
pub enum UpdateError {
    Git(git2::Error),
    Io(io::Error),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            UpdateError::Git(ref err) => write!(f, "GIT error: {}", err),
            UpdateError::Io(ref err) => write!(f, "IO error: {}", err),
        }
    }
}

impl error::Error for UpdateError {
    fn cause(&self) -> Option<&dyn error::Error> {
        match *self {
            UpdateError::Git(ref err) => Some(err),
            UpdateError::Io(ref err) => Some(err),
        }
    }
}

impl From<git2::Error> for UpdateError {
    fn from(err: git2::Error) -> Self { UpdateError::Git(err) }
}

impl From<io::Error> for UpdateError {
    fn from(err: io::Error) -> Self { UpdateError::Io(err) }
}

impl Repo {
    pub fn new(name: String, config: Config) -> Self {
        return Self {
            name,
            config,

            state: Mutex::new(RepoState {
                last_checked: None,
                last_changed: None,
            }),
        };
    }

    pub fn update(&self) -> Result<bool, UpdateError> {
        let now = Some(Instant::now());

        self.state.lock().unwrap().last_checked = now;

        let path = Path::new(&self.config.path);

        let repository: git2::Repository;
        if path.exists() {
            println!("[{}] Using existing repository", self.name);

            // Open the repo or give up
            repository = git2::Repository::open(path)?;
        } else {
            println!("[{}] Initialized new repository", self.name);

            // Create the directory and init the repo
            fs::create_dir_all(path)?;
            repository = git2::Repository::init(path)?;
        }

        let mut remote = repository.remote_anonymous(&self.config.remote_url)?;

        let mut remote_cb = git2::RemoteCallbacks::new();
        remote_cb.credentials(|url, username, allowed| {
            println!("[{}] cred: url = {:?}", self.name, url);
            println!("[{}] cred: username = {:?}", self.name, username);
            println!("[{}] cred: allowed = {:?}", self.name, allowed);

            if allowed.contains(git2::CredentialType::USERNAME) {
                match self.config.credentials {
                    Some(Credentials::SSH(ref ssh)) => if let Some(ref username) = ssh.username {
                        return git2::Cred::username(username);
                    },

                    Some(Credentials::Password(ref password)) => if let Some(ref username) = password.username {
                        return git2::Cred::username(username);
                    },

                    None => return Err(git2::Error::from_str("Authentication is required"))
                }
            }

            if allowed.contains(git2::CredentialType::SSH_MEMORY) {
                if let Some(Credentials::SSH(ref ssh)) = self.config.credentials {
                    let private_key = if ssh.private_key_path {
                        let path = ssh.private_key.clone();
                        let mut file = File::open(path).map_err(|_| git2::Error::from_str("Could not open credentials file"))?;
                        let mut contents = String::new();
                        file.read_to_string(&mut contents).map_err(|_| git2::Error::from_str("Could not read credentials file"))?;
                        contents
                    } else {
                        ssh.private_key.clone()
                    };

                    return git2::Cred::ssh_key_from_memory(username.unwrap(),
                                                           ssh.public_key.as_ref().map(String::as_ref),
                                                           private_key.as_ref(),
                                                           ssh.passphrase.as_ref().map(String::as_ref));
                }
            }

            if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
                if let Some(Credentials::Password(ref password)) = self.config.credentials {
                    return git2::Cred::userpass_plaintext(username.unwrap(),
                                                          password.password.as_ref());
                }
            }

            return Err(git2::Error::from_str("Unsupported authentication"));
        });

        println!("[{}] Fetching data from remote", self.name);
        remote.fetch(&[&format!("+{}:refs/pullomatic", self.config.remote_ref())],
                     Some(git2::FetchOptions::new()
                             .prune(git2::FetchPrune::On)
                             .remote_callbacks(remote_cb)),
                     None)?;
        println!("[{}] Fetched data from remote", self.name);

//        repository.find_reference("HEAD")?;
        let latest_obj = repository.revparse_single("HEAD").ok();
        let remote_obj = repository.revparse_single("refs/pullomatic")?;

        if let Some(ref latest_obj) = latest_obj {
            if latest_obj.id() == remote_obj.id() {
                println!("[{}] Already up to date", self.name);
                return Ok(false);
            }
        }

        repository.reset(&remote_obj,
                         git2::ResetType::Hard,
                         Some(git2::build::CheckoutBuilder::new()
                                 .force()
                                 .remove_untracked(true)))?;

        println!("[{}] Updated to {}", self.name, remote_obj.id());
        self.state.lock().unwrap().last_changed = now;

        return Ok(true);
    }

    pub fn name(&self) -> &str { &self.name }

    pub fn config(&self) -> &Config { &self.config }

    pub fn last_checked(&self) -> Option<Instant> { self.state.lock().unwrap().last_checked }
}
