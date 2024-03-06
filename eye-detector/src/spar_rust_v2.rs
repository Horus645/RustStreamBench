use super::common;
use opencv::{core, objdetect, prelude::*, types, videoio};
use spar_rust_v2::*;

#[derive(Clone)]
struct MatData {
    frame: Mat,
}
unsafe impl Sync for MatData {}
unsafe impl Send for MatData {}

struct EyesData {
    frame: Mat,
    equalized: Mat,
    faces: types::VectorOfRect,
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
        .map(|frame| MatData { frame })
}

#[stage]
fn stage1(frame: MatData) -> (MatData, MatData) {
    let equalized = MatData {
        frame: common::prepare_frame(&frame.frame).unwrap(),
    };
    (frame, equalized)
}

#[stage(State(face_xml))]
fn stage2(frame: MatData, equalized: MatData, face_xml: String) -> EyesData {
    let mut face_detector = objdetect::CascadeClassifier::new(face_xml.clone().as_str()).unwrap();
    let faces = common::detect_faces(&equalized.frame, &mut face_detector).unwrap();
    EyesData {
        frame: frame.frame,
        equalized: equalized.frame,
        faces,
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
            &core::Mat::roi(&equalized, face).unwrap(),
            &mut eyes_detector,
        )
        .unwrap();
        common::draw_in_frame(&mut frame, &eyes, &face).unwrap();
    }
    MatData { frame }
}

#[sink(Ordered)]
fn sink(frame: MatData) -> MatData {
    frame
}

pub fn spar_rust_v2_eye_tracker(input_video: &String, nthreads: i32) -> opencv::Result<()> {
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
    let face_xml = core::find_file(unsafe { &super::FACE_XML_STR }, true, false)?;
    let eye_xml = core::find_file(unsafe { &super::EYE_XML_STR }, true, false)?;

    let out: Vec<MatData> = to_stream!(multithreaded: [
        source(video_in),
        (stage1(), nthreads as usize ),
        (stage2(face_xml), nthreads as usize ),
        (stage3(eye_xml), nthreads as usize ),
        sink,
    ])
    .collect();

    for frame in out {
        video_out.write(&frame.frame).unwrap();
    }
    Ok(())
}
