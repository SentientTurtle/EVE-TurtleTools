use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::sync::Arc;
use std::time::{Duration};
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Router;
use axum::routing::get;
use evestaticdata::sde::update::SdeVersion;
use tokio::sync::RwLock;
use zipslash::parse::ParseOpts;
use zipslash::repack::{RepackOpts, Repacker};
use zipslash::range_read::SliceRangeReader;

fn main() -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?;

    let repacker = Repacker::load_archive(&SliceRangeReader(include_bytes!("./empty.zip")), &ParseOpts::default())?;

    let arc = Arc::new(RwLock::new((repacker, SdeVersion::sde { buildNumber: 0, releaseDate: None })));
    let arc2 = arc.clone();

    rt.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_mins(15));
        loop {
            interval.tick().await;
            if let Ok(version) = evestaticdata::sde::update::update_sde("./sde.zip") {
                if let Ok(input) = File::open("./sde.zip") {
                    if let Ok(repacker) = Repacker::load_archive(&input, &ParseOpts::default()) {
                        let mut guard = arc.write().await;
                        let old = std::mem::replace(&mut *guard, (repacker, version));
                        drop(guard);
                        drop(old);
                    }
                }
            }
        }
    });

    rt.block_on(server(arc2))?;

    Ok(())
}

#[derive(Debug)]
struct AppState {
    pub repacker: Arc<RwLock<(Repacker, SdeVersion)>>
}

async fn server(repacker: Arc<RwLock<(Repacker, SdeVersion)>>) -> Result<(), Box<dyn Error>>{
    let state = AppState { repacker };

    let router = Router::new()
        .route("/", get(sde))
        .route("/version/", get(sde_version))
        .with_state(Arc::new(state));

    axum::serve(
        tokio::net::TcpListener::bind("0.0.0.0:3000").await?,
        router
    )
        .await?;

    Ok(())
}

// basic handler that responds with a static string
const BUFFER_PREALLOC_SIZE: usize = 4 * 1024 * 1024;
const EXPLAINER_MESSAGE: &'static [u8] = include_bytes!("./explainer.txt");
const REPACK_OPTS: RepackOpts = RepackOpts::const_default().skip_missing_files(true);

async fn sde(State(state): State<Arc<AppState>>, Query(parameters): Query<HashMap<String, String>>) -> impl IntoResponse {
    if parameters.len() == 0 {
        (StatusCode::BAD_REQUEST, [(header::CONTENT_TYPE, "text/plain"), (header::CONTENT_DISPOSITION, "inline")], Vec::from(EXPLAINER_MESSAGE))
    } else {
        let filenames = Vec::from_iter(parameters.keys());  // TODO: Make Repacker support iterator input
        let mut buffer = Vec::with_capacity(BUFFER_PREALLOC_SIZE);
        match state.repacker.read().await.0.repack(&mut buffer, &filenames, &REPACK_OPTS) {
            Ok(_) => (StatusCode::OK, [(header::CONTENT_TYPE, "application/zip"), (header::CONTENT_DISPOSITION, "attachment; filename=\"sde_repack.zip\"")], buffer),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain"), (header::CONTENT_DISPOSITION, "inline")],
                format!("{}", err).into_bytes()
            ),
        }
    }
}

async fn sde_version(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let SdeVersion::sde { buildNumber, .. } = state.repacker.read().await.1;
    (StatusCode::OK, [(header::CONTENT_TYPE, "text/plain")], buildNumber.to_string())
}