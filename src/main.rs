use std::thread;
use std::time::Duration;

use lazy_static::lazy_static;
use log::{debug, error, info};

use prometheus::{Encoder, IntGaugeVec, TextEncoder};
use speedtest_rs::error::Error;
use speedtest_rs::speedtest;

lazy_static! {
    static ref DOWNLOAD_GAUGE_VEC: IntGaugeVec =
        prometheus::register_int_gauge_vec!("download_kbps", "hoge", &["country", "host"]).unwrap();
    static ref UPLOAD_GAUGE_VEC: IntGaugeVec =
        prometheus::register_int_gauge_vec!("upload_kbps", "hoge", &["country", "host"]).unwrap();
}

use hyper::{
    header::CONTENT_TYPE,
    server::Server,
    service::{make_service_fn, service_fn},
    Body, Request, Response,
};

use structopt::StructOpt;

#[cfg(test)]
pub mod test;

#[derive(Debug, StructOpt)]
#[structopt(name = "speedtest-exporter")]
struct Opt {
    #[structopt(env, default_value = "9100")]
    speedtest_exporter_port: u16,

    #[structopt(env, default_value = "300")]
    speedtest_interval: u64,
}

#[tokio::main]
async fn main() {
    std::env::set_var("RUST_LOG", "speedtest_exporter=info");
    env_logger::init();

    let opt = Opt::from_args();

    let addr = ([0, 0, 0, 0], opt.speedtest_exporter_port).into();
    info!("Listening on http://{}", addr);

    thread::spawn(move || loop {
        debug!("start measure");
        match measure_all() {
            Ok(_) => info!("all measure success"),
            Err(err) => match err {
                Error::Reqwest(req) => {
                    if req.is_connect() {
                        error!("connect error");
                    } else if req.is_timeout() {
                        error!("timeout error");
                    }
                    error!("{:?}", req);

                    DOWNLOAD_GAUGE_VEC.reset();
                    UPLOAD_GAUGE_VEC.reset();
                }
                _ => error!("error: {:?}", err),
            },
        }

        thread::sleep(Duration::from_secs(opt.speedtest_interval));
    });

    let serve_future = Server::bind(&addr).serve(make_service_fn(|_| async {
        Ok::<_, hyper::Error>(service_fn(serve_req))
    }));

    if let Err(err) = serve_future.await {
        error!("server error: {}", err);
    }
}

fn measure_all() -> Result<(), speedtest_rs::error::Error> {
    let config = speedtest::get_configuration()?;
    let srv_list = speedtest::get_server_list_with_config(&config)?;
    let mut srv_list = srv_list.servers_sorted_by_distance(&config);
    srv_list.truncate(5);

    for srv in &srv_list {
        let dl = measure_download(srv, &DOWNLOAD_GAUGE_VEC)?;
        let ul = measure_upload(srv, &UPLOAD_GAUGE_VEC)?;
        info!(
            "{}: download={}Mbps, upload={}Mbps",
            srv.host,
            dl as f64 / 1000.0,
            ul as f64 / 1000.0
        );
    }

    Ok(())
}

async fn serve_req(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let encoder = TextEncoder::new();

    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    let response = Response::builder()
        .status(200)
        .header(CONTENT_TYPE, encoder.format_type())
        .body(Body::from(buffer))
        .unwrap();

    Ok(response)
}

//fn main_old() {
//    let mut buffer = vec![];
//    let encoder = TextEncoder::new();
//    loop {
//        let metric_families = prometheus::gather();
//        encoder.encode(&metric_families, &mut buffer).unwrap();
//
//        //println!("{:?}", metric_families);
//        println!("{}", String::from_utf8(buffer.clone()).unwrap());
//        buffer.clear();
//    }
//}

fn measure_download(
    srv: &speedtest::SpeedTestServer,
    gauge: &IntGaugeVec,
) -> Result<i64, speedtest_rs::error::Error> {
    let mut config = speedtest::get_configuration()?;
    let download = speedtest::test_download_with_progress_and_config(srv, || {}, &mut config);

    let dl;
    if let Ok(download) = download {
        dl = download.kbps() as i64;
    } else {
        dl = 0;
    }

    gauge.with_label_values(&[&srv.country, &srv.host]).set(dl);
    Ok(dl)
}

fn measure_upload(
    srv: &speedtest::SpeedTestServer,
    gauge: &IntGaugeVec,
) -> Result<i64, speedtest_rs::error::Error> {
    let mut config = speedtest::get_configuration()?;
    let upload = speedtest::test_upload_with_progress_and_config(srv, || {}, &mut config);

    let ul;
    if let Ok(upload) = upload {
        ul = upload.kbps() as i64;
    } else {
        ul = 0;
    }

    gauge.with_label_values(&[&srv.country, &srv.host]).set(ul);
    Ok(ul)
}
