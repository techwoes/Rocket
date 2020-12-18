#[macro_use] extern crate rocket;

use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};

use rocket::{Request, State, Data, Response};
use rocket::fairing::{AdHoc, Fairing, Info, Kind};
use rocket::http::{Method, ContentType, Status};

struct Token(i64);

#[cfg(test)] mod tests;

#[derive(Default)]
struct Counter {
    get: AtomicUsize,
    post: AtomicUsize,
}

#[rocket::async_trait]
impl Fairing for Counter {
    fn info(&self) -> Info {
        Info {
            name: "GET/POST Counter",
            kind: Kind::Request | Kind::Response
        }
    }

    async fn on_request(&self, request: &mut Request<'_>, _: &mut Data) {
        if request.method() == Method::Get {
            self.get.fetch_add(1, Ordering::Relaxed);
        } else if request.method() == Method::Post {
            self.post.fetch_add(1, Ordering::Relaxed);
        }
    }

    async fn on_response<'r>(&self, req: &'r Request<'_>, res: &mut Response<'r>) {
        if res.status() != Status::NotFound {
            return
        }

        if req.method() == Method::Get && req.uri().path() == "/counts" {
            let get_count = self.get.load(Ordering::Relaxed);
            let post_count = self.post.load(Ordering::Relaxed);

            let body = format!("Get: {}\nPost: {}", get_count, post_count);
            res.set_status(Status::Ok);
            res.set_header(ContentType::Plain);
            res.set_sized_body(body.len(), Cursor::new(body));
        }
    }
}

#[put("/")]
fn hello() -> &'static str {
    "Hello, world!"
}

#[get("/token")]
fn token(token: State<'_, Token>) -> String {
    format!("{}", token.0)
}

#[launch]
fn rocket() -> rocket::Rocket {
    rocket::ignite()
        .mount("/", routes![hello, token])
        .attach(Counter::default())
        .attach(AdHoc::on_attach("Token State", |rocket| async {
            println!("Adding token managed state...");
            match rocket.figment().extract_inner("token") {
                Ok(value) => Ok(rocket.manage(Token(value))),
                Err(_) => Err(rocket)
            }
        }))
        .attach(AdHoc::on_launch("Launch Message", |_| {
            println!("Rocket is about to launch!");
        }))
        .attach(AdHoc::on_request("PUT Rewriter", |req, _| {
            Box::pin(async move {
                println!("    => Incoming request: {}", req);
                if req.uri().path() == "/" {
                    println!("    => Changing method to `PUT`.");
                    req.set_method(Method::Put);
                }
            })
        }))
        .attach(AdHoc::on_response("Response Rewriter", |req, res| {
            Box::pin(async move {
                if req.uri().path() == "/" {
                    println!("    => Rewriting response body.");
                    res.set_sized_body(None, Cursor::new("Hello, fairings!"));
                }
            })
        }))
}
