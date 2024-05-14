use std::{cmp::Reverse, collections::BinaryHeap};

use super::common;
use opencv::{
    core,
    imgcodecs::{imdecode, imencode},
    objdetect,
    prelude::*,
    videoio,
};
use serde::{de::Visitor, ser::SerializeStruct, Deserialize, Serialize};

use spar_rust_v2::mpi::{
    self,
    traits::{Communicator, Destination, Source},
};

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

impl PartialEq for MatData {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl std::cmp::Eq for MatData {}
impl std::cmp::PartialOrd for MatData {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MatData {
    fn cmp(&self, _other: &Self) -> std::cmp::Ordering {
        std::cmp::Ordering::Equal
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
                Ok(MatData(imdecode(&bmp_buf, opencv::imgcodecs::IMREAD_COLOR).unwrap()))
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

pub fn mpi_eye_tracker(input_video: &String, nthreads: i32) -> opencv::Result<()> {
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

    let threads = 1 + nthreads as usize * 3;
    let (universe, _threading) = mpi::initialize_with_threading(mpi::Threading::Multiple).unwrap();
    let world = universe.world();
    let size = world.size() as usize;

    if size < threads {
        panic!("trying to execute with {threads} workers, but only have {size}");
    }

    let rank = world.rank();

    if rank as usize >= threads {
        std::process::exit(unsafe { mpi::ffi::MPI_Finalize() });
    }

    if rank == 0 {
        std::thread::spawn(move || {
            let mut sequence_number = 0u32;
            let comm = mpi::topology::SimpleCommunicator::world();
            let mut target_rank = 1;
            loop {
                // Read frame
                let mut frame = Mat::default();
                video_in.read(&mut frame).unwrap();
                if frame.size().unwrap().width == 0 {
                    break;
                }
                let frame = MatData(frame);

                let bytes = bincode::serialize(&frame).unwrap();
                let size = bytes.len() as u32;
                let target = comm.process_at_rank(target_rank);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
                target.send(&sequence_number);
                sequence_number += 1;

                target_rank += 1;
                if target_rank as usize > threads / 3 {
                    target_rank = 1;
                }
            }
            for i in 1..(1 + threads / 3) {
                let target = comm.process_at_rank(i as i32);
                target.send(&0u32.to_ne_bytes());
            }
        });

        let mut out = Vec::new();
        let mut zeros = threads / 3;
        let recver = world.any_process();
        let mut out_of_order = BinaryHeap::new();
        let mut cur_order = 0;
        while zeros > 0 {
            let (size, status) = recver.receive::<u32>();
            if size == 0 {
                zeros -= 1;
                continue;
            }
            let mut buf = vec![0u8; size as usize];
            let _status = world
                .process_at_rank(status.source_rank())
                .receive_into(&mut buf);
            let (sequence_number, _status) =
                world.process_at_rank(status.source_rank()).receive::<u32>();

            let frame: MatData = bincode::deserialize(&buf).unwrap();
            if cur_order == sequence_number {
                cur_order += 1;
                out.push(frame);
            } else {
                out_of_order.push(Reverse((sequence_number, frame)));
                while let Some(Reverse((i, b))) = out_of_order.pop() {
                    if i == cur_order {
                        cur_order += 1;
                        out.push(b);
                        continue;
                    }
                    out_of_order.push(Reverse((i, b)));
                    break;
                }
            }
        }

        for frame in out {
            video_out.write(&frame.0).unwrap();
        }

        return Ok(());
    } else if rank as usize > 0 && rank as usize <= threads / 3 {
        let begin = 1 + (threads / 3);
        let end = 2 * (threads / 3);

        let mut target = (rank as usize % (1 + end - begin)) + begin;
        let mut zeros = threads / 3;
        let recver = world.any_process();
        let sender = mpi::topology::SimpleCommunicator::world();
        while zeros > 0 {
            let (size, status) = recver.receive::<u32>();
            if size == 0 {
                zeros -= 1;
                continue;
            }
            let mut buf = vec![0u8; size as usize];
            let _status = world
                .process_at_rank(status.source_rank())
                .receive_into(&mut buf);
            let (sequence_number, _status) =
                world.process_at_rank(status.source_rank()).receive::<u32>();

            let frame: MatData = bincode::deserialize(&buf).unwrap();
            let equalized = MatData(common::prepare_frame(&frame.0).unwrap());

            {
                let bytes = bincode::serialize(&(frame, equalized)).unwrap();
                let size = bytes.len() as u32;
                let target = sender.process_at_rank(target as i32);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
                target.send(&sequence_number);
            }

            target += 1;
            if target > end {
                target = begin;
            }
        }

        for target in begin..(end + 1) {
            let target = sender.process_at_rank(target as i32);
            target.send(&0u32.to_ne_bytes());
        }
    } else if rank as usize > threads / 3 && rank as usize <= 2 * (threads / 3) {
        let begin = 1 + 2 * (threads / 3);
        let end = 3 * (threads / 3);

        let mut target = (rank as usize % (1 + end - begin)) + begin;
        let mut zeros = threads / 3;
        let recver = world.any_process();
        let sender = mpi::topology::SimpleCommunicator::world();
        while zeros > 0 {
            let (size, status) = recver.receive::<u32>();
            if size == 0 {
                zeros -= 1;
                continue;
            }
            let mut buf = vec![0u8; size as usize];
            let _status = world
                .process_at_rank(status.source_rank())
                .receive_into(&mut buf);
            let (sequence_number, _status) =
                world.process_at_rank(status.source_rank()).receive::<u32>();
            let (frame, equalized): (MatData, MatData) = bincode::deserialize(&buf).unwrap();
            let mut face_detector =
                objdetect::CascadeClassifier::new(face_xml.clone().as_str()).unwrap();
            let faces = common::detect_faces(&equalized.0, &mut face_detector).unwrap();
            let eyes_data = EyesData {
                frame,
                equalized,
                faces: unsafe { std::mem::transmute(faces.to_vec()) },
            };

            {
                let bytes = bincode::serialize(&eyes_data).unwrap();
                let size = bytes.len() as u32;
                let target = sender.process_at_rank(target as i32);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
                target.send(&sequence_number);
            }

            target += 1;
            if target > end {
                target = begin;
            }
        }

        for target in begin..(end + 1) {
            let target = sender.process_at_rank(target as i32);
            target.send(&0u32.to_ne_bytes());
        }
    } else {
        let target = 0;
        let mut zeros = threads / 3;
        let recver = world.any_process();
        let sender = mpi::topology::SimpleCommunicator::world();
        while zeros > 0 {
            let (size, status) = recver.receive::<u32>();
            if size == 0 {
                zeros -= 1;
                continue;
            }
            let mut buf = vec![0u8; size as usize];
            let _status = world
                .process_at_rank(status.source_rank())
                .receive_into(&mut buf);
            let (sequence_number, _status) =
                world.process_at_rank(status.source_rank()).receive::<u32>();

            let eyes_data: EyesData = bincode::deserialize(&buf).unwrap();
            let mut eyes_detector =
                objdetect::CascadeClassifier::new(eye_xml.clone().as_str()).unwrap();
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
                common::draw_in_frame(&mut frame.0, &eyes, &unsafe { std::mem::transmute(face) })
                    .unwrap();
            }
            let mat = MatData(frame.0);
            {
                let bytes = bincode::serialize(&mat).unwrap();
                let size = bytes.len() as u32;
                let target = sender.process_at_rank(target);
                target.send(&size.to_ne_bytes());
                target.send(&bytes);
                target.send(&sequence_number);
            }
        }
        let target = sender.process_at_rank(target);
        target.send(&0u32.to_ne_bytes());
    }

    drop(universe);
    std::process::exit(0);
}
