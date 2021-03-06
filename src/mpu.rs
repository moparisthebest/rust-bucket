use std::io::prelude::*;
use std::fs::File;
use std::path::Path;
use std::io;

use std::io::{Read, Cursor};
use rocket::data::{self, FromData};
use rocket::Data;
use rocket::Outcome;
use rocket::request::Request;
use rocket::http::Status;
use multipart::server::Multipart;

pub struct MultipartUpload {
    file: Vec<u8>,
}

impl MultipartUpload {
    pub fn stream_to_file(&self, path: &Path) -> io::Result<()> {
        let mut f = File::create(path)?;
        f.write_all(&self.file)
    }
}

fn get_multipart(request: &Request, data: Data) -> Option<Multipart<Cursor<Vec<u8>>>> {
    // All of these errors should be reported
    let ct = request.headers().get_one("Content-Type")?;
    let idx = ct.find("boundary=")?;
    let boundary = &ct[(idx + "boundary=".len())..];

    let mut d = Vec::new();
    data.stream_to(&mut d).ok()?;

    Some(Multipart::with_body(Cursor::new(d), boundary))
}

impl FromData for MultipartUpload {
    type Error = ();

    fn from_data(request: &Request, data: Data) -> data::Outcome<Self, Self::Error> {
        let mut mp = match get_multipart(&request, data) {
            Some(m) => m,
            None => return Outcome::Failure((Status::raw(421), ())),
        };

        // Custom implementation parts
        let mut file = None;

        mp.foreach_entry(|mut entry| match entry.headers.name.as_str() {
            "file" => {
                println!("filename: {:?}", entry.headers.filename);
                let mut d = Vec::new();
                entry.data.read_to_end(&mut d).expect("cant read");
                file = Some(d);
            }
            other => panic!("No known key {}", other),
        }).expect("Unable to iterate");

        Outcome::Success(MultipartUpload {
            file: file.expect("file not set"),
        })
    }
}
