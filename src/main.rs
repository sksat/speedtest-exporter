use std::thread;
use std::time::Duration;

use lazy_static::lazy_static;
use prometheus::{Encoder, IntGaugeVec, TextEncoder};
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

#[tokio::main]
async fn main() {
    let addr = ([0, 0, 0, 0], 9100).into();
    println!("Listening on http://{}", addr);

    thread::spawn(move || loop {
        let config = speedtest::get_configuration().unwrap();
        let srv_list = speedtest::get_server_list_with_config(&config).unwrap();
        let mut srv_list = srv_list.servers_sorted_by_distance(&config);
        srv_list.truncate(5);

        for srv in &srv_list {
            measure_download(srv, &DOWNLOAD_GAUGE_VEC);
            measure_upload(srv, &UPLOAD_GAUGE_VEC);
        }

        thread::sleep(Duration::from_secs(5 * 60));
    });

    let serve_future = Server::bind(&addr).serve(make_service_fn(|_| async {
        Ok::<_, hyper::Error>(service_fn(serve_req))
    }));

    if let Err(err) = serve_future.await {
        eprintln!("server error: {}", err);
    }
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

fn measure_download(srv: &speedtest::SpeedTestServer, gauge: &IntGaugeVec) {
    let mut config = speedtest::get_configuration().unwrap();
    let download = speedtest::test_download_with_progress_and_config(srv, || {}, &mut config);
    let download = download.unwrap().kbps();
    gauge
        .with_label_values(&[&srv.country, &srv.host])
        .set(download as i64);
}

fn measure_upload(srv: &speedtest::SpeedTestServer, gauge: &IntGaugeVec) {
    let mut config = speedtest::get_configuration().unwrap();
    let upload = speedtest::test_upload_with_progress_and_config(srv, || {}, &mut config);
    let upload = upload.unwrap().kbps();
    gauge
        .with_label_values(&[&srv.country, &srv.host])
        .set(upload as i64);
}
