use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{response::Html, routing::get, Router};
use chrono::NaiveDate;
use minijinja::{context, Environment};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::fs::{self, DirEntry};
use std::sync::Arc;

struct AppState {
    env: Environment<'static>,
    posts: Vec<Post>,
}

const POSTS_DIR: &str = "./content/posts/";

#[tokio::main]
async fn main() {
    let env = get_jenv();
    let posts = read_posts();

    let app_state = Arc::new(AppState { env, posts: posts });

    let app = Router::new()
        .route("/", get(homepage))
        .route("/p/:slug", get(get_posts))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

fn get_jenv() -> Environment<'static> {
    let mut env = Environment::new();
    env.add_template("layout", include_str!("./templates/layout.jinja"))
        .unwrap();
    env.add_template("home", include_str!("./templates/home.jinja"))
        .unwrap();
    env.add_template("post", include_str!("./templates/post.jinja"))
        .unwrap();
    env
}

fn read_posts() -> Vec<Post> {
    let post_files = fs::read_dir(POSTS_DIR).expect("Invalid content directory");
    let mut posts: Vec<Post> = post_files
        .map(|file| file.unwrap().try_into().unwrap())
        .collect();
    posts.sort_by_key(|p| Reverse(p.meta.date));
    posts
}

async fn homepage(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let template = state.env.get_template("home").unwrap();

    let rendered = template
        .render(context! {
            title => "Ankush Menat's Blog",
            posts => state.posts,
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
                })
                .unwrap();

            Ok(Html(rendered))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Debug, Serialize)]
struct Post {
    slug: String,
    raw_content: String,
    meta: PostMeta,
}

#[derive(Debug, Deserialize, Serialize)]
struct PostMeta {
    title: String,
    external_url: Option<String>,
    date: NaiveDate,
}

impl From<DirEntry> for Post {
    fn from(value: DirEntry) -> Self {
        let file_path = value.file_name().to_str().unwrap().to_string();
        let slug = file_path.strip_suffix(".md").unwrap().to_string();
        let raw_content =
            fs::read_to_string(value.path()).expect("Content should be present in file");

        let sections: Vec<_> = raw_content.split("---").collect();
        let frontmatter = sections[1];
        let meta = serde_yaml::from_str(&frontmatter).expect("Invalid Frontmatter");

        Post {
            slug,
            raw_content,
            meta,
        }
    }
}
