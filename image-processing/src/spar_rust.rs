use raster::filter;
use raster::Image;
use std::time::SystemTime;

use spar_rust::to_stream;

pub fn spar_rust(dir_name: &str, threads: usize) {
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

    let _ = to_stream!(OUTPUT(()), {
        for image in all_images {
            STAGE(INPUT(image: Image), OUTPUT(Image), REPLICATE = threads, {
                filter::saturation(&mut image, 0.2).unwrap();
                Some(image)
            });
            STAGE(INPUT(image: Image), OUTPUT(Image), REPLICATE = threads, {
                filter::emboss(&mut image).unwrap();
                Some(image)
            });
            STAGE(INPUT(image: Image), OUTPUT(Image), REPLICATE = threads, {
                filter::gamma(&mut image, 2.0).unwrap();
                Some(image)
            });
            STAGE(INPUT(image: Image), OUTPUT(Image), REPLICATE = threads, {
                filter::sharpen(&mut image).unwrap();
                Some(image)
            });
            STAGE(INPUT(image: Image), OUTPUT(()), REPLICATE = threads, {
                filter::grayscale(&mut image).unwrap();
                Some(())
            });
        }
    });

    let system_duration = start.elapsed().expect("Failed to get render time?");
    let in_sec = system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
    println!("Execution time: {} sec", in_sec);
}
