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
/// See `FilesSecretAuth::new` for more information
pub struct FilesSecretAuth {
    folder: PathBuf,
}

/// Returns a new FilesSecretAuth object.
/// The path argument is where the authenticator will attempt to locate user files
/// a user file for user <u> with ClientId <c> and secret <s> is defined as:
///     a plaintext file named <u> (no extension), with contents: `<c>$<s>`
///     for example: file 'alice' containing text '0$alice_secret'
impl FilesSecretAuth {
    pub fn new(path: &Path) -> Self {
        FilesSecretAuth {
            folder: PathBuf::from(path),
        }
    }

    pub fn set_path(&mut self, new_path: &Path) {
        self.folder = PathBuf::from(new_path);
    }

    pub fn get_path(&self) -> &Path { &self.folder }
}

impl Authenticator for FilesSecretAuth {
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

/// Authenticator object that stores the userbase in memory as a simple HashMap.
/// The map's structure corresponds with the nature of the `Authenticator`
/// trait's function `identity_and_secret`, with username acting as a key, and 
/// the tuple (ClientId, secret) acting as a value.
pub struct MapAuth {
    map: HashMap<String, (ClientId, String)>,
}

impl MapAuth {
    /// Returns a new MapAuth From a given vector with elements (u,s)
    /// as username and secret respectively. The index of the element
    /// determines its ClientId (0 maps to ClientId(0))
    pub fn new_from_vec(users: Vec<(String, String)>) -> Self {
        let mut map = HashMap::new();
        let mut num = 0;
        for (user, secret) in users {
            map.insert(user, (ClientId(num), secret));
            num += 1;
        }
        MapAuth { map: map }
    }

    /// Returns a new MapAuth with the given map.
    pub fn new_from_map(map: HashMap<String, (ClientId, String)>) -> Self {
        MapAuth { map: map }
    }

    /// Exposes the inner map for inspection
    pub fn get_inner_map(&self) -> &HashMap<String, (ClientId, String)> {
        &self.map
    }

    /// Exposes the inner map for modification
    pub fn get_mut_map(&mut self) -> &mut HashMap<String, (ClientId, String)> {
        &mut self.map
    }
}

impl Authenticator for MapAuth {
    fn identity_and_secret(&mut self, user: &str) -> Option<(ClientId, String)> {
        self.map.get(user).map(|x| x.to_owned())
    }
}