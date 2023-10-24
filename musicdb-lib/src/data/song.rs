use std::{
    fmt::Display,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

use crate::load::ToFromBytes;

use super::{
    database::{ClientIo, Database},
    AlbumId, ArtistId, CoverId, DatabaseLocation, GeneralData, SongId,
};

#[derive(Clone, Debug)]
pub struct Song {
    pub id: SongId,
    pub location: DatabaseLocation,
    pub title: String,
    pub album: Option<AlbumId>,
    pub artist: ArtistId,
    pub more_artists: Vec<ArtistId>,
    pub cover: Option<CoverId>,
    pub general: GeneralData,
    /// None => No cached data
    /// Some(Err) => No cached data yet, but a thread is working on loading it.
    /// Some(Ok(data)) => Cached data is available.
    pub cached_data: Arc<Mutex<Option<Result<Arc<Vec<u8>>, JoinHandle<Option<Arc<Vec<u8>>>>>>>>,
}
impl Song {
    pub fn new(
        location: DatabaseLocation,
        title: String,
        album: Option<AlbumId>,
        artist: ArtistId,
        more_artists: Vec<ArtistId>,
        cover: Option<CoverId>,
    ) -> Self {
        Self {
            id: 0,
            location,
            title,
            album,
            artist,
            more_artists,
            cover,
            general: GeneralData::default(),
            cached_data: Arc::new(Mutex::new(None)),
        }
    }
    pub fn uncache_data(&self) -> Result<(), ()> {
        let mut cached = self.cached_data.lock().unwrap();
        match cached.as_ref() {
            Some(Ok(_data)) => {
                *cached = None;
                Ok(())
            }
            Some(Err(_thread)) => Err(()),
            None => Ok(()),
        }
    }
    /// If no data is cached yet and no caching thread is running, starts a thread to cache the data.
    pub fn cache_data_start_thread(&self, db: &Database) -> bool {
        let mut cd = self.cached_data.lock().unwrap();
        let start_thread = match cd.as_ref() {
            None => true,
            Some(Err(_)) | Some(Ok(_)) => false,
        };
        if start_thread {
            let src = if let Some(dlcon) = &db.remote_server_as_song_file_source {
                Err((self.id, Arc::clone(dlcon)))
            } else {
                Ok(db.get_path(&self.location))
            };
            *cd = Some(Err(std::thread::spawn(move || {
                let data = Self::load_data(src)?;
                Some(Arc::new(data))
            })));
            true
        } else {
            false
        }
    }
    /// Gets the cached data, if available.
    /// If a thread is running to load the data, it is not awaited.
    /// This function doesn't block.
    pub fn cached_data(&self) -> Option<Arc<Vec<u8>>> {
        if let Some(Ok(v)) = self.cached_data.lock().unwrap().as_ref() {
            Some(Arc::clone(v))
        } else {
            None
        }
    }
    /// Gets the cached data, if available.
    /// If a thread is running to load the data, it *is* awaited.
    /// This function will block until the data is loaded.
    /// If it still returns none, some error must have occured.
    pub fn cached_data_now(&self, db: &Database) -> Option<Arc<Vec<u8>>> {
        let mut cd = self.cached_data.lock().unwrap();
        *cd = match cd.take() {
            None => {
                let src = if let Some(dlcon) = &db.remote_server_as_song_file_source {
                    Err((self.id, Arc::clone(dlcon)))
                } else {
                    Ok(db.get_path(&self.location))
                };
                if let Some(v) = Self::load_data(src) {
                    Some(Ok(Arc::new(v)))
                } else {
                    None
                }
            }
            Some(Err(t)) => match t.join() {
                Err(_e) => None,
                Ok(Some(v)) => Some(Ok(v)),
                Ok(None) => None,
            },
            Some(Ok(v)) => Some(Ok(v)),
        };
        drop(cd);
        self.cached_data()
    }
    fn load_data(
        src: Result<
            PathBuf,
            (
                SongId,
                Arc<Mutex<crate::server::get::Client<Box<dyn ClientIo>>>>,
            ),
        >,
    ) -> Option<Vec<u8>> {
        match src {
            Ok(path) => {
                eprintln!("[info] loading song from {:?}", path);
                match std::fs::read(&path) {
                    Ok(v) => {
                        eprintln!("[info] loaded song from {:?}", path);
                        Some(v)
                    }
                    Err(e) => {
                        eprintln!("[info] error loading {:?}: {e:?}", path);
                        None
                    }
                }
            }
            Err((id, dlcon)) => {
                eprintln!("[info] loading song {id}");
                match dlcon
                    .lock()
                    .unwrap()
                    .song_file(id, true)
                    .expect("problem with downloader connection...")
                {
                    Ok(data) => Some(data),
                    Err(e) => {
                        eprintln!("[WARN] error loading song {id}: {e}");
                        None
                    }
                }
            }
        }
    }
}
impl Display for Song {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.title)?;
        match self.album {
            Some(album) => write!(f, " (by {} on {album})", self.artist)?,
            None => write!(f, " (by {})", self.artist)?,
        }
        Ok(())
    }
}

impl ToFromBytes for Song {
    fn to_bytes<T>(&self, s: &mut T) -> Result<(), std::io::Error>
    where
        T: Write,
    {
        self.id.to_bytes(s)?;
        self.location.to_bytes(s)?;
        self.title.to_bytes(s)?;
        self.album.to_bytes(s)?;
        self.artist.to_bytes(s)?;
        self.more_artists.to_bytes(s)?;
        self.cover.to_bytes(s)?;
        self.general.to_bytes(s)?;
        Ok(())
    }
    fn from_bytes<T>(s: &mut T) -> Result<Self, std::io::Error>
    where
        T: Read,
    {
        Ok(Self {
            id: ToFromBytes::from_bytes(s)?,
            location: ToFromBytes::from_bytes(s)?,
            title: ToFromBytes::from_bytes(s)?,
            album: ToFromBytes::from_bytes(s)?,
            artist: ToFromBytes::from_bytes(s)?,
            more_artists: ToFromBytes::from_bytes(s)?,
            cover: ToFromBytes::from_bytes(s)?,
            general: ToFromBytes::from_bytes(s)?,
            cached_data: Arc::new(Mutex::new(None)),
        })
    }
}
