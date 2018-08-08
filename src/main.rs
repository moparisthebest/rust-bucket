#![feature(plugin, decl_macro, custom_derive)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate adjective_adjective_animal;
//extern crate tokio;
//extern crate tokio_codec;
extern crate multipart;

mod paste_id;
mod mpu;
#[cfg(test)] mod tests;

use std::io;
use std::fs::File;
use std::path::{Path, PathBuf};

use rocket::response::NamedFile;

use rocket::Data;
use rocket::response::content;

use paste_id::PasteID;
use mpu::MultipartUpload;
use rocket::request::LenientForm;
use std::fs;

//use tokio_codec::{Decoder, BytesCodec};
//use tokio::net::TcpListener;
//use tokio::prelude::*;
use std::thread;
use std::net::TcpListener;
use std::io::{Write,Read};
use std::time::Duration;

const HOST: &'static str = "http://localhost:8000";

const UPLOAD_MAX_SIZE: u64 = 8 * 1024 * 1024;

fn new_paste() -> (String, String) {
    let id = PasteID::new();
    let filename = format!("upload/{id}", id = id);
    let url = format!("{host}/{id}\n", host = HOST, id = id);
    (filename, url)
}

fn upload(paste: Data, key: Option<PasteID>) -> io::Result<String> {
    let (filename, url) = new_paste();

    paste.stream_to_file(Path::new(&filename))?;
    Ok(url)
}

fn upload_string(paste: &str, key: Option<PasteID>) -> io::Result<String> {
    let (filename, url) = new_paste();

    fs::write(filename, paste).expect("Unable to write file");
    Ok(url)
}

#[derive(FromForm)]
struct PasteForm {
    content: String,
    extension: String,
}

// todo: change /w to /, shouldn't conflict because of format, but it does currently
#[post("/w", format = "application/x-www-form-urlencoded", data = "<paste>")]
fn web_post(paste: LenientForm<PasteForm>) -> io::Result<String> {
    upload_string(&paste.get().content, None)
}

// todo: change /w to /, shouldn't conflict because of format, but it does currently
#[post("/m", format = "multipart/form-data", data = "<paste>")]
fn mpu_post(paste: MultipartUpload) -> io::Result<String> {
    let (filename, url) = new_paste();

    paste.stream_to_file(Path::new(&filename))?;
    Ok(url)
}

#[put("/", data = "<paste>")]
fn upload_put(paste: Data) -> io::Result<String> {
    upload(paste, None)
}

#[post("/", data = "<paste>")]
fn upload_post(paste: Data) -> io::Result<String> {
    upload(paste, None)
}

#[patch("/", data = "<paste>")]
fn upload_patch(paste: Data) -> io::Result<String> {
    upload(paste, None)
}

#[put("/<key>", data = "<paste>")]
fn upload_put_key(paste: Data, key: PasteID) -> io::Result<String> {
    upload(paste, Some(key))
}

#[post("/<key>", data = "<paste>")]
fn upload_post_key(paste: Data, key: PasteID) -> io::Result<String> {
    upload(paste, Some(key))
}

#[patch("/<key>", data = "<paste>")]
fn upload_patch_key(paste: Data, key: PasteID) -> io::Result<String> {
    upload(paste, Some(key))
}

#[get("/<id>")]
fn get(id: PasteID) -> Option<content::Plain<File>> {
    let filename = format!("upload/{id}", id = id);
    File::open(&filename).map(|f| content::Plain(f)).ok()
}

#[delete("/<id>/<key>")]
fn delete(id: PasteID, key: PasteID) -> Option<content::Plain<File>> {
    let filename = format!("upload/{id}", id = id);
    File::open(&filename).map(|f| content::Plain(f)).ok()
}

#[patch("/<id>/<key>")]
fn patch(id: PasteID, key: PasteID) -> Option<content::Plain<File>> {
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
    get, delete, patch
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
        let timeout = Some(Duration::new(5, 0));
        let mut listener = TcpListener::bind("127.0.0.1:12345").unwrap();

        loop {
            match listener.accept() {
                Ok((mut stream, addr)) => {
                    thread::spawn(move || {
                        let (filename, url) = new_paste();

                        let mut paste_file = File::create(&filename).expect("cannot create file?");

                        stream.set_read_timeout(timeout).expect("set read timeout failed?");
                        stream.set_write_timeout(timeout).expect("set write timeout failed?");

                        stream.write(&url.into_bytes()).expect("write failed?");
                        stream.flush();

                        copy(&mut stream, &mut paste_file);

                        //handle_request(stream, addr);
                    })
                },
                Err(e) => {
                    thread::spawn(move || {
                        println!("Connection failed: {:?}", e)
                    })
                },
            };
        };
    });
    /*
    // Bind the server's socket
    let addr = "127.0.0.1:12345".parse().unwrap();
    let tcp = TcpListener::bind(&addr).unwrap();

    // Iterate incoming connections
    let server = tcp.incoming().for_each(|tcp| {

        let id = PasteID::new(ID_LENGTH);
        let filename = format!("upload/{id}", id = id);
        let url = format!("{host}/{id}\n", host = HOST, id = id);

        // Split up the read and write halves
        //let (reader, writer) = tcp.split();


        // Copy the data back to the client
        let conn = tokio::io::copy(reader, writer)
            // print what happened
            .map(|(n, _, _)| {
                println!("wrote {} bytes", n)
            })
            // Handle any errors
            .map_err(|err| {
                println!("IO error {:?}", err)
            });

        let conn = tokio::io::write_all(writer, url)
            .then(|res| {
                println!("wrote message; success={:?}", res.is_ok());
                Ok(())
            });


        let mut paste_file = File::create(&filename)?;

        let conn = tokio::io::re(reader, vec!(0; 4096)).then(move |res| {
            let result = match res {
                Ok((_, buf, n)) => {
                    //info!(client_logger, "persisted"; "filepath" => filepath);
                    paste_file.write(&buf[0..n]).unwrap();

                    //info!(client_logger, "replied"; "message" => url);
                    tokio::io::write_all(writer, format!("{}\n", url).as_bytes()).wait().unwrap();

                    //info!(client_logger, "finished connection");
                    Ok(())
                }
                Err(e) => {
                    //error!(client_logger, "failed to read from client");
                    Err(e)
                }
            };
            drop(result);
            Ok(())
        });

        let mut paste_file = tokio::fs::file::File::create(&filename);

        let conn = tokio::io::copy(reader, paste_file)
            // print what happened
            .map(|(n, _, _)| {
                println!("wrote {} bytes", n)
            })
            // Handle any errors
            .map_err(|err| {
                println!("IO error {:?}", err)
            });



        let framed = BytesCodec::new().framed(tcp);
        let (writer, reader) = framed.split();

        let mut paste_file = File::create(&filename)?;

        let conn = writer.write_buf(url)
            .then(|res| {
                println!("wrote message; success={:?}", res.is_ok());
                Ok(())
            });

        let conn = reader
            .for_each(move|bytes| {
                println!("bytes: {:?}", bytes);
                paste_file.write_all(&bytes).expect("cannot write to file?");
                Ok(())
            })
            // After our copy operation is complete we just print out some helpful
            // information.
            .and_then(|()| {
                println!("Socket received FIN packet and closed connection");
                Ok(())
            })
            .or_else(|err| {
                println!("Socket closed with error: {:?}", err);
                // We have to return the error to catch it in the next ``.then` call
                Err(err)
            })
            .then(|result| {
                println!("Socket closed with result: {:?}", result);
                Ok(())
            });

        // Spawn the future as a concurrent task
        tokio::spawn(conn);

        Ok(())
    })
        .map_err(|err| {
            println!("server error {:?}", err);
        });

    // Start the runtime and spin up the server
    tokio::run(server);
    */
}

fn main() {

    run_tcp();
    rocket().launch();
}
