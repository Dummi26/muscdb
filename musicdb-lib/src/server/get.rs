use std::{
    io::BufRead,
    io::{BufReader, Read, Write},
    sync::{Arc, Mutex},
};

use crate::data::{database::Database, CoverId};

pub struct Client<T: Write + Read>(BufReader<T>);
impl<T: Write + Read> Client<T> {
    pub fn new(mut con: BufReader<T>) -> std::io::Result<Self> {
        writeln!(con.get_mut(), "get")?;
        Ok(Self(con))
    }
    pub fn cover_bytes(&mut self, id: CoverId) -> Result<Result<Vec<u8>, String>, std::io::Error> {
        writeln!(
            self.0.get_mut(),
            "{}",
            con_get_encode_string(&format!("cover-bytes\n{id}"))
        )?;
        let mut response = String::new();
        self.0.read_line(&mut response)?;
        let response = con_get_decode_line(&response);
        if response.starts_with("len: ") {
            if let Ok(len) = response[4..].trim().parse() {
                let mut bytes = vec![0; len];
                self.0.read_exact(&mut bytes)?;
                Ok(Ok(bytes))
            } else {
                Ok(Err(response))
            }
        } else {
            Ok(Err(response))
        }
    }
}

pub fn handle_one_connection_as_get(
    db: Arc<Mutex<Database>>,
    connection: &mut BufReader<impl Read + Write>,
) -> Result<(), std::io::Error> {
    let mut line = String::new();
    loop {
        line.clear();
        if connection.read_line(&mut line).is_ok() {
            if line.is_empty() {
                return Ok(());
            }
            let request = con_get_decode_line(&line);
            let mut request = request.lines();
            if let Some(req) = request.next() {
                match req {
                    "cover-bytes" => {
                        if let Some(cover) = request
                            .next()
                            .and_then(|id| id.parse().ok())
                            .and_then(|id| db.lock().unwrap().covers().get(&id).cloned())
                        {
                            if let Some(v) = cover.get_bytes(
                                |p| db.lock().unwrap().get_path(p),
                                |bytes| {
                                    writeln!(connection.get_mut(), "len: {}", bytes.len())?;
                                    connection.get_mut().write_all(bytes)?;
                                    Ok::<(), std::io::Error>(())
                                },
                            ) {
                                v?;
                            } else {
                                writeln!(connection.get_mut(), "no data")?;
                            }
                        } else {
                            writeln!(connection.get_mut(), "no cover")?;
                        }
                    }
                    _ => {}
                }
            }
        } else {
            return Ok(());
        }
    }
}

pub fn con_get_decode_line(line: &str) -> String {
    let mut o = String::new();
    let mut chars = line.chars();
    loop {
        match chars.next() {
            Some('\\') => match chars.next() {
                Some('n') => o.push('\n'),
                Some('r') => o.push('\r'),
                Some('\\') => o.push('\\'),
                Some(ch) => o.push(ch),
                None => break,
            },
            Some(ch) => o.push(ch),
            None => break,
        }
    }
    o
}
pub fn con_get_encode_string(line: &str) -> String {
    let mut o = String::new();
    for ch in line.chars() {
        match ch {
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            _ => o.push(ch),
        }
    }
    o
}
