use super::common;
use futures::{future::lazy, stream, task::Poll, StreamExt};
use opencv::{core, objdetect, prelude::*, types, videoio};
use tokio::sync::oneshot;

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

macro_rules! spawn_return {
    ($block:expr) => {{
        let (sender, receiver) = oneshot::channel::<_>();
        tokio::spawn(lazy(move |_| {
            let result = $block;
            sender.send(result).ok();
        }));
        receiver
    }};
}

pub fn tokio_eye_tracker(input_video: &String, nthreads: i32) -> opencv::Result<()> {
    let mut video_in = videoio::VideoCapture::from_file(input_video, videoio::CAP_FFMPEG)?;
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
    let mut video_out: videoio::VideoWriter =
        videoio::VideoWriter::new("output.avi", fourcc, fps_out, frame_size, true)?;
    let out_opened = videoio::VideoWriter::is_opened(&video_out)?;
    if !out_opened {
        panic!("Unable to open output video output.avi!");
    }

    let start = std::time::SystemTime::now();

    let processing_stream = stream::poll_fn(move |_| -> Poll<Option<MatData>> {
        // Read frame
        let mut frame = Mat::default();
        video_in.read(&mut frame).unwrap();
        if frame.size().unwrap().width == 0 {
            return Poll::Ready(None);
        }
        Poll::Ready(Some(MatData { frame }))
    });

    let threads = nthreads as usize;

    let pipeline = processing_stream
        .map(move |in_data: MatData| {
            spawn_return!({
                let face_xml =
                    core::find_file(unsafe { super::FACE_XML_STR.as_str() }, true, false).unwrap();
                let mut face_detector = objdetect::CascadeClassifier::new(&face_xml).unwrap();

                let equalized = common::prepare_frame(&in_data.frame).unwrap();

                // Detect faces
                let faces = common::detect_faces(&equalized, &mut face_detector).unwrap();
                // Out data
                EyesData {
                    frame: in_data.frame,
                    equalized,
                    faces,
                }
            })
        })
        .buffered(threads)
        .map(move |in_data| {
            let mut in_data = in_data.unwrap();
            spawn_return!({
                let eye_xml =
                    core::find_file(unsafe { super::EYE_XML_STR.as_str() }, true, false).unwrap();
                let mut eye_detector = objdetect::CascadeClassifier::new(&eye_xml).unwrap();

                for face in in_data.faces {
                    let eyes = common::detect_eyes(
                        &core::Mat::roi(&in_data.equalized, face)
                            .unwrap()
                            .clone_pointee(),
                        &mut eye_detector,
                    )
                    .unwrap();

                    common::draw_in_frame(&mut in_data.frame, &eyes, &face).unwrap();
                }
                MatData {
                    frame: in_data.frame,
                }
            })
        })
        .buffered(threads)
        .for_each(move |in_data| {
            let in_data = in_data.unwrap();
            video_out.write(&in_data.frame).unwrap();
            futures::future::ready(())
        });

    let system_duration = start.elapsed().expect("Failed to get render time?");
    let in_sec = system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
    println!("Execution time: {in_sec} sec");

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(pipeline);

    Ok(())
}
