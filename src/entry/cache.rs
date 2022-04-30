use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use async_std::fs::{File, OpenOptions};
use async_std::io::{ReadExt, WriteExt};
use async_std::sync::RwLock;
use async_std::task::block_on;
use once_cell::sync::OnceCell;
use serde::{Serialize, Deserialize};

fn get_default_cache_path() -> &'static str {
    static CACHE: OnceCell<String> = OnceCell::new();
    CACHE.get_or_init(|| {
        dotenv::var("CACHE_DEFAULT")
            .unwrap_or_else(|_| String::from("./.cache.json"))
    })
}

#[derive(Debug, Clone)]
pub struct CacheHandler {
    path: String,
    cache: Arc<RwLock<HashSet<Cache>>>
}

impl CacheHandler {
    pub fn load_from<P>(path: P) -> CacheHandler where P: Into<String> + Clone {
        CacheHandler {
            path: path.clone().into(),
            cache: Arc::new(RwLock::new(serde_json::from_str(&block_on(CacheHandler::read(path.into()))).unwrap_or_default()))
        }
    }

    pub async fn abs(&self, cache: Cache) {
        self.remove(cache.as_ref_key()).await;
        self.push(cache).await;
    }

    pub async fn push(&self, cache: Cache) {
        self.cache.write().await.insert(cache);
    }

    pub async fn get<K>(&self, key: K) -> Option<Etag> where K: Into<String> + Clone {
        self.cache.read().await.iter()
            .find(|temp| temp.key == key.clone().into())
            .map(|cache| cache.as_ref_value().to_owned())
    }

    pub async fn remove<K>(&self, key: K) where K: Into<String> + Clone {
        self.cache.write().await.retain(|cache| cache.key != key.to_owned().into());
    }

    async fn write(&self) {
        let mut file = CacheHandler::open(&self.path).await;
        file.set_len(0).await.expect("");
        let cache_string = serde_json::to_string(&self.cache.read().await.iter().collect::<Vec<_>>())
            .expect("cannot write to cache file.");
        let _ = file.write(cache_string.as_ref()).await;
    }

    async fn read<P>(path: P) -> String where P: AsRef<Path> {
        let mut file = CacheHandler::open(path).await;
        let mut buf = String::new();
        let _ = file.read_to_string(&mut buf).await
            .expect("read failed");
        buf
    }

    async fn open<P>(path: P) -> File where P: AsRef<Path> {
        let path = path.as_ref();
        OpenOptions::new()
            .read(true).write(true).open(path).await
                .unwrap_or_else(|_| block_on(
                OpenOptions::new().create(true)
                    .write(true).read(true).open(path))
                    .expect("cannot open"))
    }
}

impl Default for CacheHandler {
    fn default() -> Self {
        CacheHandler::load_from(get_default_cache_path())
    }
}

impl Drop for CacheHandler {
    fn drop(&mut self) {
        block_on(self.write());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Default)]
#[serde(transparent)]
pub struct Etag(String);

impl Etag {
    pub fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for Etag {
    fn from(str: String) -> Self {
        Etag(str)
    }
}

impl From<Etag> for String {
    fn from(etag: Etag) -> Self {
        etag.0
    }
}

// Todo: Cacheをさらに汎用的にする為、ジェネリクスを使用すること。
#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct Cache {
    key: String,
    value: Etag
}

impl Cache {
    pub fn new<K, V>(key: K, value: V) -> Cache
      where K: Into<String>, V: Into<String> {
        Self { key: key.into(), value: Etag::from(value.into()) }
    }

    pub fn as_ref_key(&self) -> &str {
        &self.key
    }

    pub fn as_ref_value(&self) -> &Etag {
        &self.value
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;
    use futures::StreamExt;
    use crate::entry::cache::{Cache, CacheHandler, Etag};

    #[tokio::test]
    async fn handling_test() {
        {
            let cache = CacheHandler::load_from("./.test/cache_test.json");
            cache.push(Cache::new("test", "value")).await;
            let exist_etag = cache.get("test").await;
            let no_exist = cache.get("abc").await;

            assert_eq!(exist_etag, Some(Etag::from("value".to_string())));
            assert_eq!(no_exist, None);

            cache.remove("test").await;
            let existed_etag = cache.get("test").await;
            assert_eq!(existed_etag, None);
            cache.push(Cache::new("save", "this-value")).await;
        }
        println!("cache handler dropped.");
        assert!(Path::new("./.test/cache_test.json").exists())
    }

    #[tokio::test]
    async fn thread_safe_test() {
        {
            let vector = vec![
                Cache::new("test-1", "value-1"),
                Cache::new("test-2", "value-2"),
                Cache::new("test-3", "value-3"),
                Cache::new("test-4", "value-4"),
                Cache::new("test-5", "value-5")
            ];
            let handler = CacheHandler::load_from("./.test/cache_test.json");
            futures::stream::iter(vector.iter()).map(|cache| {
                let handler = &handler;
                async move {
                    handler.push(cache.to_owned()).await;
                    cache
                }
            }).buffer_unordered(4)
              .collect::<Vec<_>>().await;
            let exist_etag = handler.get("test-1").await;
            let no_exist = handler.get("abc").await;

            assert_eq!(exist_etag, Some(Etag::from("value-1".to_string())));
            assert_eq!(no_exist, None);

            handler.remove("test-1").await;
            let existed_etag = handler.get("test-1").await;
            assert_eq!(existed_etag, None);
            handler.push(Cache::new("save", "this-value")).await;
            let overwrite = Cache::new("test-2", "this-is-overwrite");
            handler.remove(overwrite.as_ref_key()).await;
            handler.push(overwrite).await;
            let overwrite = Cache::new("test-3", "this-is-overwrite-by-abs");
            handler.abs(overwrite).await;
        }
        println!("cache handler dropped.");
        assert!(Path::new("./.test/cache_test.json").exists());
    }
}