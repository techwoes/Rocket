//! Custom handler and options for static file serving.
//!
//! See the [`StaticFiles`](crate::serve::StaticFiles) type for further details.
//!
//! # Enabling
//!
//! This module is only available when the `serve` feature is enabled. Enable it
//! in `Cargo.toml` as follows:
//!
//! ```toml
//! [dependencies.rocket_contrib]
//! version = "0.5.0-dev"
//! default-features = false
//! features = ["serve"]
//! ```

use std::path::{PathBuf, Path};

use rocket::{Request, Data, Route};
use rocket::http::{Method, uri::Segments, ext::IntoOwned};
use rocket::handler::{Handler, Outcome};
use rocket::response::{NamedFile, Redirect};

/// Generates a crate-relative version of `$path`.
///
/// This macro is primarily intended for use with [`StaticFiles`] to serve files
/// from a path relative to the crate root. The macro accepts one parameter,
/// `$path`, an absolute or relative path. It returns a path (an `&'static str`)
/// prefixed with the path to the crate root. Use `Path::new()` to retrieve an
/// `&'static Path`.
///
/// See the [relative paths `StaticFiles`
/// documentation](`StaticFiles`#relative-paths) for an example.
///
/// # Example
///
/// ```rust
/// use rocket_contrib::serve::{StaticFiles, crate_relative};
///
/// let manual = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
/// let automatic = crate_relative!("static");
/// assert_eq!(manual, automatic);
///
/// use std::path::Path;
///
/// let manual = Path::new(env!("CARGO_MANIFEST_DIR")).join("static");
/// let automatic_1 = Path::new(crate_relative!("static"));
/// let automatic_2 = Path::new(crate_relative!("/static"));
/// assert_eq!(manual, automatic_1);
/// assert_eq!(automatic_1, automatic_2);
/// ```
#[macro_export]
macro_rules! crate_relative {
    ($path:expr) => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)
    };
}

#[doc(inline)]
pub use crate_relative;

/// A bitset representing configurable options for the [`StaticFiles`] handler.
///
/// The valid options are:
///
///   * [`Options::None`] - Return only present, visible files.
///   * [`Options::DotFiles`] - In addition to visible files, return dotfiles.
///   * [`Options::Index`] - Render `index.html` pages for directory requests.
///   * [`Options::NormalizeDirs`] - Redirect directories without a trailing
///     slash to ones with a trailing slash.
///
/// `Options` structures can be `or`d together to select two or more options.
/// For instance, to request that both dot files and index pages be returned,
/// use `Options::DotFiles | Options::Index`.
#[derive(Debug, Clone, Copy)]
pub struct Options(u8);

#[allow(non_upper_case_globals, non_snake_case)]
impl Options {
    /// `Options` representing the empty set. No dotfiles or index pages are
    /// rendered. This is different than [`Options::default()`](#impl-Default),
    /// which enables `Index`.
    pub const None: Options = Options(0b0000);

    /// `Options` enabling responding to requests for a directory with the
    /// `index.html` file in that directory, if it exists. When this is enabled,
    /// the [`StaticFiles`] handler will respond to requests for a directory
    /// `/foo` with the file `${root}/foo/index.html` if it exists. This is
    /// enabled by default.
    pub const Index: Options = Options(0b0001);

    /// `Options` enabling returning dot files. When this is enabled, the
    /// [`StaticFiles`] handler will respond to requests for files or
    /// directories beginning with `.`. This is _not_ enabled by default.
    pub const DotFiles: Options = Options(0b0010);

    /// `Options` that normalizes directory requests by redirecting requests to
    /// directory paths without a trailing slash to ones with a trailing slash.
    ///
    /// When enabled, the [`StaticFiles`] handler will respond to requests for a
    /// directory without a trailing `/` with a permanent redirect (308) to the
    /// same path with a trailing `/`. This ensures relative URLs within any
    /// document served for that directory will be interpreted relative to that
    /// directory, rather than its parent. This is _not_ enabled by default.
    pub const NormalizeDirs: Options = Options(0b0100);

    /// Returns `true` if `self` is a superset of `other`. In other words,
    /// returns `true` if all of the options in `other` are also in `self`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use rocket_contrib::serve::Options;
    ///
    /// let index_request = Options::Index | Options::DotFiles;
    /// assert!(index_request.contains(Options::Index));
    /// assert!(index_request.contains(Options::DotFiles));
    ///
    /// let index_only = Options::Index;
    /// assert!(index_only.contains(Options::Index));
    /// assert!(!index_only.contains(Options::DotFiles));
    ///
    /// let dot_only = Options::DotFiles;
    /// assert!(dot_only.contains(Options::DotFiles));
    /// assert!(!dot_only.contains(Options::Index));
    /// ```
    #[inline]
    pub fn contains(self, other: Options) -> bool {
        (other.0 & self.0) == other.0
    }
}

/// The default set of options: `Options::Index`.
impl Default for Options {
    fn default() -> Self {
        Options::Index
    }
}

impl std::ops::BitOr for Options {
    type Output = Self;

    #[inline(always)]
    fn bitor(self, rhs: Self) -> Self {
        Options(self.0 | rhs.0)
    }
}

/// Custom handler for serving static files.
///
/// This handler makes it simple to serve static files from a directory on the
/// local file system. To use it, construct a `StaticFiles` using either
/// [`StaticFiles::from()`] or [`StaticFiles::new()`] then simply `mount` the
/// handler at a desired path. When mounted, the handler will generate route(s)
/// that serve the desired static files. If a requested file is not found, the
/// routes _forward_ the incoming request. The default rank of the generated
/// routes is `10`. To customize route ranking, use the [`StaticFiles::rank()`]
/// method.
///
/// # Options
///
/// The handler's functionality can be customized by passing an [`Options`] to
/// [`StaticFiles::new()`].
///
/// # Example
///
/// To serve files from the `/static` directory on the local file system at the
/// `/public` path, allowing `index.html` files to be used to respond to
/// requests for a directory (the default), you might write the following:
///
/// ```rust,no_run
/// # #[macro_use] extern crate rocket;
/// # extern crate rocket_contrib;
/// use rocket_contrib::serve::StaticFiles;
///
/// #[launch]
/// fn rocket() -> rocket::Rocket {
///     rocket::ignite().mount("/public", StaticFiles::from("/static"))
/// }
/// ```
///
/// With this, requests for files at `/public/<path..>` will be handled by
/// returning the contents of `/static/<path..>`. Requests for _directories_ at
/// `/public/<directory>` will be handled by returning the contents of
/// `/static/<directory>/index.html`.
///
/// ## Relative Paths
///
/// In the example above, `/static` is an absolute path. If your static files
/// are stored relative to your crate and your project is managed by Rocket, use
/// the [`crate_relative!`] macro to obtain a path that is relative to your
/// crate's root. For example, to serve files in the `static` subdirectory of
/// your crate at `/`, you might write:
///
/// ```rust,no_run
/// # #[macro_use] extern crate rocket;
/// # extern crate rocket_contrib;
/// use rocket_contrib::serve::{StaticFiles, crate_relative};
///
/// #[launch]
/// fn rocket() -> rocket::Rocket {
///     rocket::ignite().mount("/", StaticFiles::from(crate_relative!("/static")))
/// }
/// ```
#[derive(Clone)]
pub struct StaticFiles {
    root: PathBuf,
    options: Options,
    rank: isize,
}

impl StaticFiles {
    /// The default rank use by `StaticFiles` routes.
    const DEFAULT_RANK: isize = 10;

    /// Constructs a new `StaticFiles` that serves files from the file system
    /// `path`. By default, [`Options::Index`] is set, and the generated routes
    /// have a rank of `10`. To serve static files with other options, use
    /// [`StaticFiles::new()`]. To choose a different rank for generated routes,
    /// use [`StaticFiles::rank()`].
    ///
    /// # Panics
    ///
    /// Panics if `path` does not exist or is not a directory.
    ///
    /// # Example
    ///
    /// Serve the static files in the `/www/public` local directory on path
    /// `/static`.
    ///
    /// ```rust,no_run
    /// # #[macro_use] extern crate rocket;
    /// # extern crate rocket_contrib;
    /// use rocket_contrib::serve::StaticFiles;
    ///
    /// #[launch]
    /// fn rocket() -> rocket::Rocket {
    ///     rocket::ignite().mount("/static", StaticFiles::from("/www/public"))
    /// }
    /// ```
    ///
    /// Exactly as before, but set the rank for generated routes to `30`.
    ///
    /// ```rust,no_run
    /// # #[macro_use] extern crate rocket;
    /// # extern crate rocket_contrib;
    /// use rocket_contrib::serve::StaticFiles;
    ///
    /// #[launch]
    /// fn rocket() -> rocket::Rocket {
    ///     rocket::ignite().mount("/static", StaticFiles::from("/www/public").rank(30))
    /// }
    /// ```
    pub fn from<P: AsRef<Path>>(path: P) -> Self {
        StaticFiles::new(path, Options::default())
    }

    /// Constructs a new `StaticFiles` that serves files from the file system
    /// `path` with `options` enabled. By default, the handler's routes have a
    /// rank of `10`. To choose a different rank, use [`StaticFiles::rank()`].
    ///
    /// # Panics
    ///
    /// Panics if `path` does not exist or is not a directory.
    ///
    /// # Example
    ///
    /// Serve the static files in the `/www/public` local directory on path
    /// `/static` without serving index files or dot files. Additionally, serve
    /// the same files on `/pub` with a route rank of -1 while also serving
    /// index files and dot files.
    ///
    /// ```rust,no_run
    /// # #[macro_use] extern crate rocket;
    /// # extern crate rocket_contrib;
    /// use rocket_contrib::serve::{StaticFiles, Options};
    ///
    /// #[launch]
    /// fn rocket() -> rocket::Rocket {
    ///     let options = Options::Index | Options::DotFiles;
    ///     rocket::ignite()
    ///         .mount("/static", StaticFiles::from("/www/public"))
    ///         .mount("/pub", StaticFiles::new("/www/public", options).rank(-1))
    /// }
    /// ```
    pub fn new<P: AsRef<Path>>(path: P, options: Options) -> Self {
        use rocket::yansi::Paint;

        let path = path.as_ref();
        if !path.is_dir() {
            error!("`StaticFiles` supplied with invalid path");
            info_!("'{}' is not a directory", Paint::white(path.display()));
            panic!("refusing to continue due to invalid static files path");
        }

        StaticFiles { root: path.into(), options, rank: Self::DEFAULT_RANK }
    }

    /// Sets the rank for generated routes to `rank`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # extern crate rocket_contrib;
    /// use rocket_contrib::serve::{StaticFiles, Options};
    ///
    /// // A `StaticFiles` created with `from()` with routes of rank `3`.
    /// StaticFiles::from("/public").rank(3);
    ///
    /// // A `StaticFiles` created with `new()` with routes of rank `-15`.
    /// StaticFiles::new("/public", Options::Index).rank(-15);
    /// ```
    pub fn rank(mut self, rank: isize) -> Self {
        self.rank = rank;
        self
    }
}

impl Into<Vec<Route>> for StaticFiles {
    fn into(self) -> Vec<Route> {
        let non_index = Route::ranked(self.rank, Method::Get, "/<path..>", self.clone());
        // `Index` requires routing the index for obvious reasons.
        // `NormalizeDirs` requires routing the index so a `.mount("/foo")` with
        // a request `/foo`, can be redirected to `/foo/`.
        if self.options.contains(Options::Index) || self.options.contains(Options::NormalizeDirs) {
            let index = Route::ranked(self.rank, Method::Get, "/", self);
            vec![index, non_index]
        } else {
            vec![non_index]
        }
    }
}

async fn handle_dir<'r, P>(opt: Options, r: &'r Request<'_>, d: Data, p: P) -> Outcome<'r>
    where P: AsRef<Path>
{
    if opt.contains(Options::NormalizeDirs) && !r.uri().path().ends_with('/') {
        let new_path = r.uri().map_path(|p| p.to_owned() + "/")
            .expect("adding a trailing slash to a known good path results in a valid path")
            .into_owned();

        return Outcome::from_or_forward(r, d, Redirect::permanent(new_path));
    }

    if !opt.contains(Options::Index) {
        return Outcome::forward(d);
    }

    let file = NamedFile::open(p.as_ref().join("index.html")).await.ok();
    Outcome::from_or_forward(r, d, file)
}

#[rocket::async_trait]
impl Handler for StaticFiles {
    async fn handle<'r, 's: 'r>(&'s self, req: &'r Request<'_>, data: Data) -> Outcome<'r> {
        // If this is not the route with segments, handle it only if the user
        // requested a handling of index files.
        let current_route = req.route().expect("route while handling");
        let is_segments_route = current_route.uri.path().ends_with(">");
        if !is_segments_route {
            return handle_dir(self.options, req, data, &self.root).await;
        }

        // Otherwise, we're handling segments. Get the segments as a `PathBuf`,
        // only allowing dotfiles if the user allowed it.
        let allow_dotfiles = self.options.contains(Options::DotFiles);
        let path = req.get_segments::<Segments<'_>>(0)
            .and_then(|res| res.ok())
            .and_then(|segments| segments.into_path_buf(allow_dotfiles).ok())
            .map(|path| self.root.join(path));

        match path {
            Some(p) if p.is_dir() => handle_dir(self.options, req, data, p).await,
            Some(p) => Outcome::from_or_forward(req, data, NamedFile::open(p).await.ok()),
            None => Outcome::forward(data),
        }
    }
}
