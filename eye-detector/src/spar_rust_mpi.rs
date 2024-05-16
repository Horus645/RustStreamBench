use super::common;
use opencv::{
    core,
    imgcodecs::{imdecode, imencode},
    objdetect,
    prelude::*,
    videoio,
};
use serde::{de::Visitor, ser::SerializeStruct, Deserialize, Serialize};
use spar_rust_v2::*;

#[repr(transparent)]
#[derive(Debug)]
struct MatData(Mat);
unsafe impl Sync for MatData {}
unsafe impl Send for MatData {}

impl Serialize for MatData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut bmp_buf = core::Vector::new();
        imencode(".bmp", &self.0, &mut bmp_buf, &core::Vector::new()).unwrap();

        let mut state = serializer.serialize_struct("MatData", 1)?;
        state.serialize_field("frame", bmp_buf.as_slice())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for MatData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MatDataVisitor;

        impl<'de> Visitor<'de> for MatDataVisitor {
            type Value = MatData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct MatData")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let bytes: &[u8] = seq.next_element()?.unwrap();
                let bmp_buf = core::Vector::from_slice(bytes);
                Ok(MatData(
                    imdecode(&bmp_buf, opencv::imgcodecs::IMREAD_COLOR).unwrap(),
                ))
            }
        }
        deserializer.deserialize_struct("MatData", &["frame"], MatDataVisitor)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
struct Rect(core::Rect);

impl Serialize for Rect {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let core::Rect {
            x,
            y,
            width,
            height,
        } = &self.0;
        let mut state = serializer.serialize_struct("Rect", 4)?;
        state.serialize_field("x", x)?;
        state.serialize_field("y", y)?;
        state.serialize_field("width", width)?;
        state.serialize_field("height", height)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Rect {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RectVisitor;

        impl<'de> Visitor<'de> for RectVisitor {
            type Value = Rect;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct MatData")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let x = seq.next_element()?.unwrap();
                let y = seq.next_element()?.unwrap();
                let width = seq.next_element()?.unwrap();
                let height = seq.next_element()?.unwrap();
                Ok(Rect(core::Rect {
                    x,
                    y,
                    width,
                    height,
                }))
            }
        }
        deserializer.deserialize_struct("MatData", &["x", "y", "width", "height"], RectVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct EyesData {
    frame: MatData,
    equalized: MatData,
    faces: Vec<Rect>,
}

unsafe impl Sync for EyesData {}
unsafe impl Send for EyesData {}

#[source]
fn source(mut video_in: videoio::VideoCapture) -> impl Iterator<Item = MatData> {
    (0..)
        .map(move |_| {
            let mut frame = Mat::default();
            video_in.read(&mut frame).unwrap();
            frame
        })
        .take_while(|frame| frame.size().unwrap().width > 0)
        .map(MatData)
}

#[stage]
fn stage1(frame: MatData) -> (MatData, MatData) {
    let equalized = MatData(common::prepare_frame(&frame.0).unwrap());
    (frame, equalized)
}

#[stage(State(face_xml))]
fn stage2(frame: MatData, equalized: MatData, face_xml: String) -> EyesData {
    let mut face_detector = objdetect::CascadeClassifier::new(face_xml.clone().as_str()).unwrap();
    let faces = common::detect_faces(&equalized.0, &mut face_detector).unwrap();
    EyesData {
        frame,
        equalized,
        faces: unsafe { std::mem::transmute(faces.to_vec()) },
    }
}

#[stage(State(eye_xml))]
fn stage3(eyes_data: EyesData, eye_xml: String) -> MatData {
    let mut eyes_detector = objdetect::CascadeClassifier::new(eye_xml.clone().as_str()).unwrap();
    let EyesData {
        mut frame,
        equalized,
        faces,
    } = eyes_data;
    for face in faces {
        let eyes = common::detect_eyes(
            &core::Mat::roi(&equalized.0, unsafe { std::mem::transmute(face) })
                .unwrap()
                .clone_pointee(),
            &mut eyes_detector,
        )
        .unwrap();
        common::draw_in_frame(&mut frame.0, &eyes, &unsafe { std::mem::transmute(face) }).unwrap();
    }
    MatData(frame.0)
}

#[sink(Ordered)]
fn sink(frame: MatData) -> MatData {
    frame
}

pub fn spar_rust_mpi_eye_tracker(input_video: &String, nthreads: i32) -> opencv::Result<()> {
    let video_in = videoio::VideoCapture::from_file(input_video, videoio::CAP_FFMPEG)?;
    let in_opened = videoio::VideoCapture::is_opened(&video_in)?;
    if !in_opened {
        panic!("Unable to open input video {input_video}!");
    }
    let frame_size = core::Size::new(
        video_in.get(videoio::VideoCaptureProperties::CAP_PROP_FRAME_WIDTH as i32)? as i32,
        video_in.get(videoio::VideoCaptureProperties::CAP_PROP_FRAME_HEIGHT as i32)? as i32,
    );
    let fourcc = videoio::VideoWriter::fourcc('m', 'p', 'g', '1')?;
    let fps_out = video_in.get(videoio::VideoCaptureProperties::CAP_PROP_FPS as i32)?;
    let mut video_out = videoio::VideoWriter::new("output.avi", fourcc, fps_out, frame_size, true)?;
    let out_opened = videoio::VideoWriter::is_opened(&video_out)?;
    if !out_opened {
        panic!("Unable to open output video output.avi!");
    }

    //"haarcascade_frontalface_alt.xml".to_owned()
    let face_xml = core::find_file(unsafe { super::FACE_XML_STR.as_str() }, true, false)?;
    let eye_xml = core::find_file(unsafe { super::EYE_XML_STR.as_str() }, true, false)?;

    let start = std::time::SystemTime::now();

    let out: Vec<MatData> = to_stream!(mpi: [
        source(video_in),
        (stage1(), nthreads as usize ),
        (stage2(face_xml), nthreads as usize ),
        (stage3(eye_xml), nthreads as usize ),
        sink,
    ])
    .0
    .collect();
    let system_duration = start.elapsed().expect("Failed to get render time?");
    let in_sec = system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
    println!("Execution time: {in_sec} sec");

    for frame in out {
        video_out.write(&frame.0).unwrap();
    }
    Ok(())
}
