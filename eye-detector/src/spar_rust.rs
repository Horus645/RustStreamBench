use super::common;
use opencv::{core, objdetect, prelude::*, types, videoio};
use spar_rust::to_stream;

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

pub fn spar_rust_eye_tracker(input_video: &String, nthreads: i32) -> opencv::Result<()> {
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
    let mut video_out = videoio::VideoWriter::new("output.avi", fourcc, fps_out, frame_size, true)?;
    let out_opened = videoio::VideoWriter::is_opened(&video_out)?;
    if !out_opened {
        panic!("Unable to open output video output.avi!");
    }

    //"haarcascade_frontalface_alt.xml".to_owned()
    let face_xml = core::find_file(unsafe { super::FACE_XML_STR.as_str() }, true, false)?;
    let eye_xml = core::find_file(unsafe { super::EYE_XML_STR.as_str() }, true, false)?;

    let start = std::time::SystemTime::now();

    let mut out: Vec<MatData> = Vec::new();
    to_stream!(
        INPUT(face_xml: String, eye_xml: String, out: Vec<MatData>),
        {
            loop {
                // Read frame
                let mut frame = Mat::default();
                video_in.read(&mut frame)?;
                if frame.size()?.width == 0 {
                    break;
                }
                let frame = MatData { frame };

                // Convert to gray and equalize frame
                STAGE(
                    INPUT(frame: MatData),
                    OUTPUT(frame: MatData, equalized: MatData),
                    REPLICATE = nthreads,
                    {
                        let equalized = MatData {
                            frame: common::prepare_frame(&frame.frame).unwrap(),
                        };
                    },
                );

                // Detect faces
                STAGE(
                    INPUT(frame: MatData, equalized: MatData, face_xml: String),
                    OUTPUT(eyes_data: EyesData),
                    REPLICATE = nthreads,
                    {
                        let mut face_detector =
                            objdetect::CascadeClassifier::new(face_xml).unwrap();
                        let faces =
                            common::detect_faces(&equalized.frame, &mut face_detector).unwrap();
                        let eyes_data = EyesData {
                            frame: frame.frame,
                            equalized: equalized.frame,
                            faces,
                        };
                    },
                );

                STAGE(
                    INPUT(eyes_data: EyesData, eye_xml: String),
                    OUTPUT(frame: MatData),
                    REPLICATE = nthreads,
                    {
                        let mut eyes_detector = objdetect::CascadeClassifier::new(eye_xml).unwrap();
                        let EyesData {
                            mut frame,
                            equalized,
                            faces,
                        } = eyes_data;
                        for face in faces {
                            let eyes = common::detect_eyes(
                                &core::Mat::roi(&equalized, face).unwrap().clone_pointee(),
                                &mut eyes_detector,
                            )
                            .unwrap();
                            common::draw_in_frame(&mut frame, &eyes, &face).unwrap();
                        }
                        let frame = MatData { frame };
                    },
                );
                //Write output frame
                STAGE(INPUT(frame: MatData, out: Vec<MatData>), ORDERED, {
                    out.push(frame);
                });
            }
        }
    );
    let system_duration = start.elapsed().expect("Failed to get render time?");
    let in_sec = system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
    println!("Execution time: {in_sec} sec");

    for frame in out {
        video_out.write(&frame.frame).unwrap();
    }
    Ok(())
}
