#![feature(plugin, decl_macro, custom_derive)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate adjective_adjective_animal;
extern crate multipart;
extern crate toml;
#[macro_use]
extern crate serde_derive;

mod paste_id;
mod mpu;
#[cfg(test)] mod tests;

use std::io;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::thread;
use std::net::TcpListener;
use std::io::{Write,Read};
use std::time::{Duration, SystemTime};

use rocket::response::NamedFile;

use rocket::Data;
use rocket::response::content;
use rocket::request::{self, Request, FromRequest, State, LenientForm};
use rocket::outcome::Outcome::*;

use paste_id::PasteID;
use mpu::MultipartUpload;

const HOST: &'static str = "http://localhost:8000";

const UPLOAD_MAX_SIZE: u64 = 8 * 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),

    Toml(toml::ser::Error),

    TomlDe(toml::de::Error),

    /// The uinput file could not be found.
    NotFound,

    /// error reading input_event
    ShortRead,
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error::Io(value)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(value: toml::ser::Error) -> Self {
        Error::Toml(value)
    }
}

impl From<toml::de::Error> for Error {
    fn from(value: toml::de::Error) -> Self {
        Error::TomlDe(value)
    }
}

#[derive(Deserialize, Serialize, Default, Debug)]
struct PasteInfo<'a> {
    // these never change
    key: Option<PasteID<'a>>, // key to update/delete paste with
    delete_after: Option<SystemTime>, // delete after this date regardless
    delete_after_num_views: Option<u32>, // delete after this many views
    delete_if_not_viewed_in_last_seconds: Option<Duration>, // delete if last_viewed is longer than this many seconds ago
    // these are updated if the above values require it
    last_viewed: Option<SystemTime>, // Only Some if delete_if_not_viewed_in_last_seconds is Some
    num_views: u32, // Only incremented if delete_after_num_views is Some, otherwise 0
}

impl<'a> PasteInfo<'a> {
    fn read<P: AsRef<Path>>(path: P) -> ::std::result::Result<PasteInfo<'static>,Error> {
        let mut f = File::open(path)?;
        let mut input = String::new();
        f.read_to_string(&mut input)?;
        let paste_info = toml::from_str(&input)?;
        Ok(paste_info)
    }

    fn write<P: AsRef<Path>>(&self, path: P) -> ::std::result::Result<(),Error> {
        let toml = toml::to_string(&self)?;
        fs::write(path, toml)?;
        Ok(())
    }

    fn should_delete(&self) -> bool {
        self.delete_after.map(|s| s >= SystemTime::now()).unwrap_or(false)
            || self.delete_after_num_views.map(|n| n >= self.num_views).unwrap_or(false)
            // self.last_viewed.unwrap() is safe because this is always Some if delete_if_not_viewed_in_last_seconds is
            || self.delete_if_not_viewed_in_last_seconds.map(|n| (SystemTime::now() - n) > self.last_viewed.unwrap()).unwrap_or(false)
    }

    fn mark_viewed_and_write<P: AsRef<Path>>(&mut self, path: P) -> ::std::result::Result<(),Error> {
        let mut must_write = false;
        if self.delete_after_num_views.is_some() {
            must_write = true;
            self.num_views += 1;
        }
        if self.delete_if_not_viewed_in_last_seconds.is_some() {
            must_write = true;
            self.last_viewed = Some(SystemTime::now());
        }
        if must_write {
            self.write(&path)?;
        }
        Ok(())
    }
}

/*
trait Backend {
    fn new_paste(&self) -> (String, String);

    fn upload(&self, paste: Data, _key: Option<PasteID>) -> io::Result<String>;

    fn upload_string(&self, paste: &str, _key: Option<PasteID>) -> io::Result<String>;
}

#[derive(Default)]
struct DefaultBackend{}

impl Backend for DefaultBackend {
    fn new_paste(&self) -> (String, String) {
        loop {
            let id = PasteID::new();
            let filename = format!("upload/{id}", id = id);
            if !Path::new(&filename).exists() {
                let url = format!("{host}/{id}\n", host = HOST, id = id);
                return (filename, url)
            }
        }
    }

    fn upload(&self, paste: Data, _key: Option<PasteID>) -> io::Result<String> {
        let (filename, url) = self.new_paste();

        paste.stream_to_file(Path::new(&filename))?;
        Ok(url)
    }

    fn upload_string(&self, paste: &str, _key: Option<PasteID>) -> io::Result<String> {
        let (filename, url) = self.new_paste();

        fs::write(filename, paste)?;
        Ok(url)
    }
}
*/

enum Backend {
    PlainFile
}

impl Backend {
    fn new_paste(&self) -> (String, String, String) {
        loop {
            let id = PasteID::new();
            let filename = format!("upload/{id}", id = id);
            if !Path::new(&filename).exists() && fs::create_dir_all(filename).is_ok() {
                let url = format!("{host}/{id}\n", host = HOST, id = id);
                return (format!("upload/{id}/file", id = id), format!("upload/{id}/info", id = id), url)
            }
        }
    }

    fn file_paths(&self, id: PasteID) -> (String, String, String) {
        (format!("upload/{id}/file", id = id), format!("upload/{id}/info", id = id), format!("upload/{id}", id = id))
    }

    fn upload(&self, paste: Data, _key: Option<PasteID>) -> io::Result<String> {
        let (filename, info_filename, url) = self.new_paste();

        paste.stream_to_file(Path::new(&filename))?;
        Ok(url)
    }

    fn upload_multipart(&self, paste: MultipartUpload) -> io::Result<String> {
        let (filename, info_filename, url) = self.new_paste();
        paste.stream_to_file(Path::new(&filename))?;
        Ok(url)
    }

    fn upload_string(&self, paste: &PasteForm) -> ::std::result::Result<String,Error> {
        let (filename, info_filename, url) = self.new_paste();
        PasteInfo::from(paste).write(info_filename)?;
        fs::write(filename, &paste.content)?;
        Ok(url)
    }

    fn get(&self, id: PasteID) -> ::std::result::Result<content::Plain<File>,Error> {
        let (filename, info_filename, foldername) = self.file_paths(id);
        let mut paste_info = PasteInfo::read(&info_filename)?;
        // first check if we should delete this
        if paste_info.should_delete() {
            fs::remove_dir_all(foldername)?;
            return Err(Error::NotFound);
        }
        // now check if we need to modify+write this
        paste_info.mark_viewed_and_write(info_filename)?;
        let file = File::open(&filename).map(|f| content::Plain(f))?;
        Ok(file)
    }
}

impl<'a, 'r> FromRequest<'a, 'r> for &'a Backend {
    type Error = ();

    fn from_request(req: &'a Request<'r>) -> request::Outcome<Self, ()> {
        let backend = req.guard::<State<Backend>>()?;
        Success(backend.inner())
    }
}

#[derive(FromForm)]
struct PasteForm {
    content: String,
    _extension: Option<String>,
    key: Option<String>, // key to update/delete paste with // todo: use PasteId here for validation if you can figure out lifetime shit
    delete_after: Option<String>, // delete after this date regardless // todo: use custom type here for validation
    delete_after_num_views: Option<u32>, // delete after this many views
    delete_if_not_viewed_in_last_seconds: Option<u64>, // delete if last_viewed is longer than this many seconds ago // todo: use Duration here for validation
}

impl<'a> From<&'a PasteForm> for PasteInfo<'a> {
    fn from(value: &'a PasteForm) -> PasteInfo<'a> {
        PasteInfo {
            key: None,//value.key.map(|s| PasteID::of(&s)),
            delete_after: None,
            delete_after_num_views: value.delete_after_num_views,
            delete_if_not_viewed_in_last_seconds: value.delete_if_not_viewed_in_last_seconds.map(|s| Duration::from_secs(s)),
            last_viewed: value.delete_if_not_viewed_in_last_seconds.map(|_s| SystemTime::now()),
            num_views: 0,
        }
    }
}

// todo: change /w to /, shouldn't conflict because of format, but it does currently
#[post("/w", format = "application/x-www-form-urlencoded", data = "<paste>")]
fn web_post(backend: &Backend, paste: LenientForm<PasteForm>) -> ::std::result::Result<String,Error> {
    backend.upload_string(paste.get())
}

// todo: change /w to /, shouldn't conflict because of format, but it does currently
#[post("/m", format = "multipart/form-data", data = "<paste>")]
fn mpu_post(backend: &Backend, paste: MultipartUpload) -> io::Result<String> {
    backend.upload_multipart(paste)
}

#[put("/", data = "<paste>")]
fn upload_put(backend: &Backend, paste: Data) -> io::Result<String> {
    backend.upload(paste, None)
}

#[post("/", data = "<paste>")]
fn upload_post(backend: &Backend, paste: Data) -> io::Result<String> {
    backend.upload(paste, None)
}

#[patch("/", data = "<paste>")]
fn upload_patch(backend: &Backend, paste: Data) -> io::Result<String> {
    backend.upload(paste, None)
}

#[put("/<key>", data = "<paste>")]
fn upload_put_key(backend: &Backend, paste: Data, key: PasteID) -> io::Result<String> {
    backend.upload(paste, Some(key))
}

#[post("/<key>", data = "<paste>")]
fn upload_post_key(backend: &Backend, paste: Data, key: PasteID) -> io::Result<String> {
    backend.upload(paste, Some(key))
}

#[patch("/<key>", data = "<paste>")]
fn upload_patch_key(backend: &Backend, paste: Data, key: PasteID) -> io::Result<String> {
    backend.upload(paste, Some(key))
}

#[get("/<id>")]
fn get(backend: &Backend, id: PasteID) -> Option<content::Plain<File>> {
    backend.get(id).ok()
}

#[delete("/<id>/<_key>")]
fn delete(id: PasteID, _key: PasteID) -> Option<content::Plain<File>> {
    let filename = format!("upload/{id}", id = id);
    File::open(&filename).map(|f| content::Plain(f)).ok()
}

#[get("/")]
fn index() -> io::Result<NamedFile> {
    NamedFile::open("static/index.html")
}

#[get("/static/<file..>")]
fn files(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(file)).ok()
}

fn rocket() -> rocket::Rocket {
    rocket::ignite().mount("/", routes![
    index, files,
    web_post,
    mpu_post,
    upload_post, upload_put, upload_patch,
    upload_post_key, upload_put_key, upload_patch_key,
    get, delete
    ])
}

// adapted from io::copy
fn copy<R: ?Sized, W: ?Sized>(reader: &mut R, writer: &mut W) -> io::Result<u64>
    where R: Read, W: Write
{
    let mut buf : [u8; 8192] = [0; 8192];

    let mut written = 0;
    loop {
        let len = match reader.read(&mut buf) {
            Ok(0) => return Ok(written),
            Ok(len) => len,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        writer.write_all(&buf[..len])?;
        written += len as u64;
        if written > UPLOAD_MAX_SIZE {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidData))
        }
    }
}

fn run_tcp(){
    // Bind the server's socket
    thread::spawn(|| {
        let backend = &Backend::PlainFile;
        let timeout = Some(Duration::new(5, 0));
        let listener = TcpListener::bind("127.0.0.1:12345").unwrap();

        loop {
            match listener.accept() {
                Ok((mut stream, _addr)) => {
                    thread::spawn(move || {
                        let (filename, _info_filename, url) = backend.new_paste();

                        let mut paste_file = File::create(&filename).expect("cannot create file?");

                        stream.set_read_timeout(timeout).expect("set read timeout failed?");
                        stream.set_write_timeout(timeout).expect("set write timeout failed?");

                        stream.write(&url.into_bytes()).expect("write failed?");
                        stream.flush().expect("can't flush?");

                        copy(&mut stream, &mut paste_file).expect("copy failed?");

                        //handle_request(stream, addr);
                    });
                },
                Err(_e) => {
                    // just ignore this I guess? could log?
                },
            };
        };
    });
}

fn main() {
    let backend = Backend::PlainFile;
    run_tcp();
    rocket().manage(backend).launch();
}
