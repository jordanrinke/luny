/*! @toon
purpose: Sample Rust fixture for testing the luny Rust parser.
    This file contains various Rust constructs including structs, enums,
    traits, and impl blocks to verify extraction works correctly.

when-editing:
    - !Keep all pub visibility modifiers represented for comprehensive testing
    - Maintain the mix of sync and async patterns (if async-std/tokio were included)

invariants:
    - Exported items must have the pub modifier
    - Private items should not be extracted as exports
    - Include examples of traits, structs, enums, and functions

do-not:
    - Remove any exports without updating corresponding tests
    - Use unsafe code in this fixture unless testing unsafe extraction

gotchas:
    - Rust uses explicit pub modifier for visibility
    - Trait implementations are extracted differently from inherent impls
    - Generic bounds affect signature extraction
*/

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

/// Exported type alias
pub type UserId = String;

/// Exported result type
pub type Result<T> = std::result::Result<T, Error>;

/// Public constants
pub const VERSION: &str = "1.0.0";
pub const DEFAULT_TIMEOUT: u64 = 30;
pub const MAX_RETRIES: u32 = 3;

// Private constant
const INTERNAL_BUFFER_SIZE: usize = 1024;

/// Exported error enum
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

/// Exported trait
pub trait Repository<T> {
    fn get(&self, id: &str) -> Result<Option<T>>;
    fn save(&self, item: &T) -> Result<()>;
    fn delete(&self, id: &str) -> Result<()>;
    fn list(&self) -> Result<Vec<T>>;
}

/// Exported struct with derive macros
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    pub id: UserId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

impl UserConfig {
    /// Public constructor
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            email: None,
            settings: HashMap::new(),
        }
    }

    /// Public builder method
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Public method
    pub fn add_setting(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.settings.insert(key.into(), value);
    }

    // Private validation method
    fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            return Err(Error::Validation("User ID is required".into()));
        }
        if self.name.is_empty() {
            return Err(Error::Validation("User name is required".into()));
        }
        Ok(())
    }
}

/// Exported service struct
pub struct UserService {
    data_dir: PathBuf,
    cache: Arc<RwLock<HashMap<String, UserConfig>>>,
}

impl UserService {
    /// Public constructor
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Public method to clear cache
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    // Private helper method
    fn get_file_path(&self, id: &str) -> PathBuf {
        self.data_dir.join(format!("{}.json", id))
    }
}

impl Repository<UserConfig> for UserService {
    fn get(&self, id: &str) -> Result<Option<UserConfig>> {
        // Check cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some(user) = cache.get(id) {
                return Ok(Some(user.clone()));
            }
        }

        // Read from file
        let path = self.get_file_path(id);
        if !path.exists() {
            return Ok(None);
        }

        let mut file = File::open(&path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let user: UserConfig = serde_json::from_str(&contents)?;

        // Update cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(id.to_string(), user.clone());
        }

        Ok(Some(user))
    }

    fn save(&self, user: &UserConfig) -> Result<()> {
        user.validate()?;

        fs::create_dir_all(&self.data_dir)?;

        let path = self.get_file_path(&user.id);
        let json = serde_json::to_string_pretty(user)?;

        let mut file = File::create(&path)?;
        file.write_all(json.as_bytes())?;

        // Update cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(user.id.clone(), user.clone());
        }

        Ok(())
    }

    fn delete(&self, id: &str) -> Result<()> {
        let path = self.get_file_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }

        // Remove from cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.remove(id);
        }

        Ok(())
    }

    fn list(&self) -> Result<Vec<UserConfig>> {
        if !self.data_dir.exists() {
            return Ok(Vec::new());
        }

        let mut users = Vec::new();
        for entry in fs::read_dir(&self.data_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Some(user) = self.get(stem)? {
                        users.push(user);
                    }
                }
            }
        }
        Ok(users)
    }
}

/// Exported factory function
pub fn create_user(name: impl Into<String>) -> UserConfig {
    UserConfig::new(generate_id(), name)
}

/// Exported validation function
pub fn validate_email(email: &str) -> Result<bool> {
    if email.is_empty() {
        return Err(Error::Validation("Email is required".into()));
    }
    Ok(email.contains('@') && email.contains('.'))
}

// Private helper function
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    format!("{}", duration.as_nanos())
}

/// Exported generic struct
pub struct Cache<K, V> {
    data: HashMap<K, V>,
    max_size: usize,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Cache<K, V> {
    pub fn new(max_size: usize) -> Self {
        Self {
            data: HashMap::new(),
            max_size,
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.data.get(key)
    }

    pub fn set(&mut self, key: K, value: V) {
        if self.data.len() >= self.max_size {
            // Remove a random entry (first one in iteration order)
            if let Some(k) = self.data.keys().next().cloned() {
                self.data.remove(&k);
            }
        }
        self.data.insert(key, value);
    }
}

/// Exported enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserStatus {
    Active,
    Inactive,
    Pending,
    Suspended,
}

impl Default for UserStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Exported macro
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        println!("[INFO] {}", format!($($arg)*));
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_config_new() {
        let user = UserConfig::new("123", "Test User");
        assert_eq!(user.id, "123");
        assert_eq!(user.name, "Test User");
        assert!(user.email.is_none());
    }
}
