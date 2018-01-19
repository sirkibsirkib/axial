use super::*;
use std::path::{PathBuf,Path};
use std::fs::File;
use std::io::prelude::*;
use std::collections::HashMap;

/// Authenticator object that checks a directory for a file at runtime.
/// Allows the server admin to change the userbase at runtime, as user data
/// is never stored in memory.
/// 
/// Initialized with the path to the directory where it finds its user files.
/// See `FilesPasswordAuth::new` for more information
pub struct FilesPasswordAuth {
    folder: PathBuf,
}

/// Returns a new FilesPasswordAuth object.
/// The path argument is where the authenticator will attempt to locate user files
/// a user file for user <u> with ClientId <c> and secret <s> is defined as:
///     a plaintext file named <u> (no extension), with contents: `<c>$<s>`
///     for example: file 'alice' containing text '0$alice_secret'
impl FilesPasswordAuth {
    pub fn new(path: &Path) -> Self {
        FilesPasswordAuth {
            folder: PathBuf::from(path),
        }
    }

    pub fn change_path(&mut self, new_path: &Path) {
        self.folder = PathBuf::from(new_path);
    }
}

impl Authenticator for FilesPasswordAuth {
    // folder containing a file per user. the name is the same as the user
    // the contents of a file are `<clientId>$<secret>`
    // eg:  file `alice` contains `4$alice_pass`
    fn identity_and_secret(&mut self, user: &str) -> Option<(ClientId, String)> {
        let mut buf = PathBuf::from(&self.folder);
        buf.push(user);
        if let Ok(mut f) = File::open(buf.as_path()) {
            let mut contents = String::new();
            if f.read_to_string(&mut contents).is_ok() {
                let mut split: Vec<&str> = contents.split("$").collect();
                if split.len() != 2 {
                    return None;
                }
                if let Ok(num) = split[0].parse::<u32>() {
                    return Some((ClientId(num), split[1].to_owned()))
                }
            }
        }
        None
    }
}

////////////////////////////////////////////////////////////////////////////////

pub struct PasswordAuth {
    map: HashMap<String, (ClientId, String)>,
}

impl PasswordAuth {
    pub fn new_from_vec(users: Vec<(String, String)>) -> Self {
        let mut map = HashMap::new();
        let mut num = 0;
        for (user, secret) in users {
            map.insert(user, (ClientId(num), secret));
            num += 1;
        }
        PasswordAuth { map: map }
    }

    pub fn new_from_map(map: HashMap<String, (ClientId, String)>) -> Self {
        PasswordAuth { map: map }
    }

    pub fn get_inner_map(&self) -> &HashMap<String, (ClientId, String)> {
        &self.map
    }

    pub fn get_mut_map(&mut self) -> &mut HashMap<String, (ClientId, String)> {
        &mut self.map
    }
}

impl Authenticator for PasswordAuth {
    fn identity_and_secret(&mut self, user: &str) -> Option<(ClientId, String)> {
        self.map.get(user).map(|x| x.to_owned())
    }
}