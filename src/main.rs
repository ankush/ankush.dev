use axum::extract::State;
use axum::http::StatusCode;
use axum::{response::Html, routing::get, Router};
use minijinja::{context, Environment};
use std::sync::Arc;

struct AppState {
    env: Environment<'static>,
}

#[tokio::main]
async fn main() {
    let env = get_jenv();

    let app_state = Arc::new(AppState { env });

    let app = Router::new()
        .route("/", get(homepage))
        .route("/p", get(get_posts))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
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

async fn homepage(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let template = state.env.get_template("home").unwrap();

    let rendered = template
        .render(context! {
            title => "Ankush Menat",
            welcome_text => "Hello World!",
        })
        .unwrap();

    Ok(Html(rendered))
}

async fn get_posts(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let template = state.env.get_template("post").unwrap();

    let some_example_entries = vec!["Data 1", "Data 2", "Data 3"];

    let rendered = template
        .render(context! {
            title => "Content",
            entries => some_example_entries,
        })
        .unwrap();

    Ok(Html(rendered))
}
