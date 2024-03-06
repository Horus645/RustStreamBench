use raster::filter;
use raster::Image;
use std::time::SystemTime;

use spar_rust_v2::*;

#[source]
#[inline]
fn source(images: Vec<Image>) -> impl Iterator<Item = Image> {
    images.into_iter()
}

#[stage]
fn stage1(mut img: Image) -> Image {
    filter::saturation(&mut img, 0.2).unwrap();
    img
}

#[stage]
fn stage2(mut img: Image) -> Image {
    filter::emboss(&mut img).unwrap();
    img
}

#[stage]
fn stage3(mut img: Image) -> Image {
    filter::gamma(&mut img, 2.0).unwrap();
    img
}

#[stage]
fn stage4(mut img: Image) -> Image {
    filter::sharpen(&mut img).unwrap();
    img
}

#[stage]
fn stage5(mut img: Image) -> Image {
    filter::grayscale(&mut img).unwrap();
    img
}

#[sink]
fn sink(_: Image) {
    // noop
}

pub fn spar_rust_v2(dir_name: &str, threads: usize) {
    let dir_entries = std::fs::read_dir(dir_name);
    let mut all_images: Vec<Image> = Vec::new();

    for entry in dir_entries.unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension().is_none() {
            continue;
        }
        all_images.push(raster::open(path.to_str().unwrap()).unwrap());
    }

    let start = SystemTime::now();

    let _: Vec<()> = to_stream!(multithreaded: [
        source(all_images),
        (stage1(), threads),
        (stage2(), threads),
        (stage3(), threads),
        (stage4(), threads),
        (stage5(), threads),
        sink,
    ])
    .collect();

    let system_duration = start.elapsed().expect("Failed to get render time?");
    let in_sec = system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
    println!("Execution time: {in_sec} sec");
}
