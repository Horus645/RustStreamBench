use raster::filter;
use serde::{de::Visitor, ser::SerializeStruct, Deserialize, Serialize};
use std::time::SystemTime;

use spar_rust_v2::*;

#[derive(Debug)]
#[repr(transparent)]
struct Image(raster::Image);

impl Serialize for Image {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let raster::Image {
            width,
            height,
            bytes,
        } = &self.0;

        let mut state = serializer.serialize_struct("Image", 3)?;
        state.serialize_field("width", width)?;
        state.serialize_field("height", height)?;
        state.serialize_field("bytes", bytes)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Image {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ImageVisitor;

        impl<'de> Visitor<'de> for ImageVisitor {
            type Value = Image;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct Image")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let width = seq.next_element()?.unwrap();
                let height = seq.next_element()?.unwrap();
                let bytes = seq.next_element()?.unwrap();
                Ok(Image(raster::Image {
                    width,
                    height,
                    bytes,
                }))
            }
        }

        deserializer.deserialize_struct("Image", &["width", "height", "bytes"], ImageVisitor)
    }
}

#[source]
fn source(images: Vec<Image>) -> impl Iterator<Item = Image> {
    images.into_iter()
}

#[stage]
fn stage1(mut img: Image) -> Image {
    filter::saturation(&mut img.0, 0.2).unwrap();
    img
}

#[stage]
fn stage2(mut img: Image) -> Image {
    filter::emboss(&mut img.0).unwrap();
    img
}

#[stage]
fn stage3(mut img: Image) -> Image {
    filter::gamma(&mut img.0, 2.0).unwrap();
    img
}

#[stage]
fn stage4(mut img: Image) -> Image {
    filter::sharpen(&mut img.0).unwrap();
    img
}

#[stage]
fn stage5(mut img: Image) -> Image {
    filter::grayscale(&mut img.0).unwrap();
    img
}

#[sink]
fn sink(img: Image) -> Image {
    img
}

pub fn spar_rust_mpi(dir_name: &str, threads: usize) {
    let dir_entries = std::fs::read_dir(dir_name);
    let mut all_images: Vec<Image> = Vec::new();

    for entry in dir_entries.unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension().is_none() {
            continue;
        }
        all_images.push(Image(raster::open(path.to_str().unwrap()).unwrap()));
    }

    let start = SystemTime::now();

    let _: Vec<Image> = to_stream!(mpi: [
        source(all_images),
        (stage1(), threads),
        (stage2(), threads),
        (stage3(), threads),
        (stage4(), threads),
        (stage5(), threads),
        sink,
    ])
    .0
    .collect();

    let system_duration = start.elapsed().expect("Failed to get render time?");
    let in_sec = system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
    println!("Execution time: {in_sec} sec");
}
