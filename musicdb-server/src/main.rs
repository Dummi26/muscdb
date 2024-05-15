#[cfg(feature = "website")]
mod web;

use std::{
    net::SocketAddr,
    path::PathBuf,
    process::exit,
    sync::{Arc, Mutex},
};

use clap::Parser;
use musicdb_lib::server::run_server_caching_thread_opt;

use musicdb_lib::data::database::Database;

#[derive(Parser, Debug)]
struct Args {
    /// The directory which contains information about the songs in your library
    #[arg()]
    db_dir: PathBuf,
    /// The path containing your actual library.
    #[arg()]
    lib_dir: PathBuf,
    /// skip reading the dbfile (because it doesn't exist yet)
    #[arg(long)]
    init: bool,
    /// optional address for tcp connections to the server
    #[arg(long)]
    tcp: Option<SocketAddr>,
    /// optional address on which to start a website which can be used on devices without `musicdb-client` to control playback.
    /// requires the `assets/` folder to be present!
    #[arg(long)]
    web: Option<SocketAddr>,

    /// allow clients to access files in this directory, or the lib_dir if not specified.
    #[arg(long)]
    custom_files: Option<Option<PathBuf>>,

    /// Use an extra background thread to cache more songs ahead of time. Useful for remote filesystems or very slow disks. If more than this many MiB of system memory are available, cache more songs.
    #[arg(long, value_name = "max_avail_mem_in_mib")]
    advanced_cache: Option<u64>,
    /// Only does something if `--advanced-cache` is used. If available system memory drops below this amount (in MiB), remove songs from cache.
    #[arg(long, value_name = "min_avail_mem_in_mib", default_value_t = 1024)]
    advanced_cache_min_mem: u64,
    /// Only does something if `--advanced-cache` is used. CacheManager will cache the current, next, ..., songs in the queue, but at most this many songs.
    #[arg(long, value_name = "number_of_songs", default_value_t = 10)]
    advanced_cache_song_lookahead_limit: u32,
}

fn main() {
    // parse args
    let args = Args::parse();
    let mut database = if args.init {
        Database::new_empty_in_dir(args.db_dir, args.lib_dir)
    } else {
        match Database::load_database_from_dir(args.db_dir.clone(), args.lib_dir.clone()) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Couldn't load database!");
                eprintln!("  dbfile: {:?}", args.db_dir);
                eprintln!("  libdir: {:?}", args.lib_dir);
                eprintln!("  err: {}", e);
                exit(1);
            }
        }
    };
    database.custom_files = args.custom_files;
    // database can be shared by multiple threads using Arc<Mutex<_>>
    let database = Arc::new(Mutex::new(database));
    if args.tcp.is_some() || args.web.is_some() {
        let mem_min = args.advanced_cache_min_mem;
        let cache_limit = args.advanced_cache_song_lookahead_limit;
        let args_tcp = args.tcp;
        let run_server = move |database, sender_sender| {
            run_server_caching_thread_opt(
                database,
                args_tcp,
                sender_sender,
                args.advanced_cache.map(|max| {
                    Box::new(
                        move |cm: &mut musicdb_lib::data::cache_manager::CacheManager| {
                            cm.set_memory_mib(mem_min, max.max(mem_min + 128));
                            cm.set_cache_songs_count(cache_limit);
                        },
                    ) as _
                }),
            );
        };
        if let Some(addr) = &args.web {
            #[cfg(not(feature = "website"))]
            {
                let _ = addr;
                eprintln!("Website support requires the 'website' feature to be enabled when compiling the server!");
                std::process::exit(80);
            }
            #[cfg(feature = "website")]
            {
                let (s, r) = std::sync::mpsc::sync_channel(1);
                let db = Arc::clone(&database);
                std::thread::spawn(move || {
                    run_server(database, Some(Box::new(move |c| s.send(c).unwrap())))
                });
                let sender = r.recv().unwrap();
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(web::main(db, sender, *addr));
            }
        } else {
            run_server(database, None);
        }
    } else {
        eprintln!("nothing to do, not starting the server.");
    }
}
