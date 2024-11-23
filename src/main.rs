use axum::extract::{Path, State, Json};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect};
use axum::{response::Html, routing::{get, post}, Router};
use chrono::NaiveDate;
use minijinja::{context, Environment};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs::{self, DirEntry};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time;
use axum_response_cache::CacheLayer;
#[allow(unused_imports)] // This is only used in debug build
use tower_http::services::ServeDir;

#[cfg(debug_assertions)]
const BASE_URL: &str = "http://localhost:3000";

#[cfg(not(debug_assertions))]
const BASE_URL: &str = "https://ankush.dev";

const RESPONSE_CACHE_TTL: u64 = 6 * 60 * 60; // =6 hour hours, any change requires a server restart anyway.

struct AppState {
    env: Environment<'static>,
    posts: Vec<Post>,
    page_hits: Mutex<HashMap<String, i64>>,
    db: Mutex<Connection>,
}

const POSTS_DIR: &str = "./content/posts/";
const DB_LOCATION: &str = "./content/data/blog.db";

#[tokio::main]
async fn main() {
    let env = get_jenv();
    let posts = read_posts();

    let app_state = Arc::new(AppState {
        env,
        posts,
        page_hits: Default::default(),
        db: Mutex::new(get_db())
    });

    restore_views(app_state.clone());
    {
        let app_state = app_state.clone();
        thread::spawn(|| {
            persistence_thread(app_state);
        });
    }
    let app = Router::new()
        .route("/", get(homepage).layer(CacheLayer::with_lifespan(RESPONSE_CACHE_TTL)))
        .route("/about", get(about))
        .route("/p/:slug", get(get_posts).layer(CacheLayer::with_lifespan(RESPONSE_CACHE_TTL)))
        .route("/:year/:month/:day/:slug", get(redirect_old_routes))
        .route("/feed.xml", get(atom_feed).layer(CacheLayer::with_lifespan(RESPONSE_CACHE_TTL)))
        .route("/pageview", post(store_pageview))
        .route("/favicon.ico", get(favicon))
        .fallback(not_found)
        .with_state(app_state.clone());

    #[cfg(debug_assertions)]
    let app = app.nest_service("/assets", ServeDir::new("./content/assets"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

fn get_jenv() -> Environment<'static> {
    let mut env = Environment::new();
    env.add_template("layout", include_str!("./templates/layout.html")).unwrap();
    env.add_template("home", include_str!("./templates/home.html")).unwrap();
    env.add_template("post", include_str!("./templates/post.html")).unwrap();
    env.add_template("feed", include_str!("./templates/feed.xml")).unwrap();
    env.add_template("style", include_str!("./templates/style.css")).unwrap();
    env.add_template("pageview", include_str!("./templates/pageview.js")).unwrap();
    env.add_function("format_date", format_date);
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);

    env
}



fn format_date(date_str: String, short: bool) -> String {
    let Ok(date) = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") else { return date_str };
    if short {
        return format!("{}", date.format("%b %d, %Y"))
    } else {
        return format!("{}", date.format("%B %d, %Y"))
    }
}

fn read_posts() -> Vec<Post> {
    let post_files = fs::read_dir(POSTS_DIR).expect("Invalid content directory");
    let mut posts: Vec<Post> = post_files.map(|file| file.unwrap().into()).collect();
    posts.sort_by_key(|p| Reverse(p.meta.date));
    posts
        .into_iter()
        .filter(|p| p.meta.published.unwrap_or(true))
        .collect()
}

async fn homepage(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let template = state.env.get_template("home").unwrap();

    let rendered = template
        .render(context! {
            title => "Blog",
            posts => state.posts,
            BASE_URL => BASE_URL,
        })
        .unwrap();

    Ok(Html(rendered))
}

async fn get_posts(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let template = state.env.get_template("post").unwrap();

    let post = state.posts.iter().find(|p| p.slug == slug);
    match post {
        Some(post) => {
            let rendered = template
                .render(context! {
                    post => post,
                    BASE_URL => BASE_URL,
                })
                .unwrap();

            Ok(Html(rendered))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "Page not found")
}

async fn redirect_old_routes(
    Path((_, _, _, slug)): Path<(String, String, String, String)>,
) -> Redirect {
    let slug = slug.strip_suffix(".html").unwrap_or(&slug);
    Redirect::permanent(&format!("/p/{slug}"))
}

async fn about() -> Redirect {
    Redirect::temporary("/")
}

async fn favicon() -> Redirect {
    Redirect::permanent("/assets/favicon.ico")
}

#[derive(Debug, Serialize)]
struct Post {
    slug: String,
    content: String,
    meta: PostMeta,
}

#[derive(Debug, Deserialize, Serialize)]
struct PostMeta {
    title: String,
    external_url: Option<String>,
    date: NaiveDate,
    iso_timestamp: Option<String>,
    description: Option<String>,
    published: Option<bool>,
}

impl From<DirEntry> for Post {
    fn from(value: DirEntry) -> Self {
        let file_path = value.file_name().to_str().unwrap().to_string();
        let slug = file_path.strip_suffix(".md").unwrap().to_string();
        let raw_content =
            fs::read_to_string(value.path()).expect("Content should be present in file");

        let sections: Vec<_> = raw_content.split("---").collect();
        let frontmatter = sections[1];
        let body = sections[2..].join("");
        let mut meta: PostMeta = serde_yaml::from_str(frontmatter).expect("Invalid Frontmatter");
        meta.iso_timestamp = Some(meta.date.format("%Y-%m-%dT00:00:00Z").to_string());

        let mut markdown_options = markdown::Options::gfm();
        markdown_options.compile.allow_dangerous_html = true;
        let content = markdown::to_html_with_options(&body, &markdown_options).unwrap();

        Post {
            slug,
            content,
            meta,
        }
    }
}

async fn atom_feed(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Reference: https://datatracker.ietf.org/doc/html/rfc4287
    let template = state.env.get_template("feed").unwrap();

    let rendered = template
        .render(context! {
            title => "Ankush Menat's Blog",
            posts => state.posts.iter().filter(|p| p.meta.external_url.is_none()).collect::<Vec<_>>(),
            author => "Ankush Menat",
            BASE_URL => BASE_URL,
        })
        .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "application/atom+xml".parse().unwrap());
    (headers, rendered)
}


#[derive(Deserialize)]
struct Pageview {
    path: String,
}

async fn store_pageview(State(state): State<Arc<AppState>>, Json(view): Json<Pageview>) {
    tokio::task::spawn_blocking(move || {
        let slug = view.path.strip_prefix("/p/").unwrap_or("");
        if !state.posts.iter().any(|p| p.slug == slug) {
            return;
        }
        let mut page_hits = state.page_hits.lock().unwrap();
        *page_hits.entry(slug.to_string()).or_default() += 1;
    });
    return ();
}


fn persistence_thread(state: Arc<AppState>) {
    loop {
        thread::sleep(time::Duration::from_secs(60));
        persist_views(state.clone());
    }
}


fn persist_views(state: Arc<AppState>) {
    let page_hits = state.page_hits.lock().unwrap();
    let db = state.db.lock().unwrap();

    page_hits.iter().for_each(|(k, v)| {
        let _ = db.execute("
            INSERT INTO page_hits (post, hits) values (?1, ?2)
                ON CONFLICT(post) DO UPDATE SET hits= ?3
            ", (k, v, v));
    })

}

fn restore_views(state: Arc<AppState>) {
    let mut page_hits = state.page_hits.lock().unwrap();
    let db = state.db.lock().unwrap();

    struct StatRow(String, i64);

    let mut query = db.prepare("select post, hits from page_hits").unwrap();

    let stat_iter = query.query_map([], |row| {
        Ok(StatRow(row.get(0)?, row.get(1)?))
    }).unwrap();

    for stat in stat_iter {
        let stat = stat.unwrap();
        page_hits.insert(stat.0, stat.1);
    }
}

fn get_db() -> Connection {
    let db = Connection::open(DB_LOCATION).unwrap();
    db.execute("
        CREATE TABLE IF NOT EXISTS page_hits (
            post TEXT PRIMARY KEY,
            hits INTEGER
        )", ()).unwrap();

    db
}
