use crate::{ble::comm_types::VideoProp, error::Result};
use anyhow::anyhow;
use gst_webrtc::WebRTCBundlePolicy;
use std::{fs::OpenOptions, io::Write, sync::mpsc, thread};
use v4l::{video::Output, Device, FourCC};

use gst::{
    glib::{self, MainLoop},
    prelude::*,
    ElementFactory, FlowReturn, Fraction, Pipeline,
};

use log::{debug, error, info};

#[derive(Debug)]
pub struct WebrtcPipeline {
    mainloop: MainLoop,
    pipeline_thread: Option<thread::JoinHandle<Result<()>>>,
    sdp_answer: String,
}

impl WebrtcPipeline {
    pub fn new(vdevice: String, sdp_offer: String, video_prop: VideoProp) -> Result<Self> {
        let mainloop = glib::MainLoop::new(None, false);

        let (tx, rx) = mpsc::channel();

        let mainloop_clone = mainloop.clone();

        info!("Creating pipeline thread");

        let pipeline_thread = thread::spawn(move || {
            match create_pipeline(mainloop_clone, vdevice, sdp_offer, tx, video_prop) {
                Ok(_) => Ok(()),
                Err(e) => {
                    error!("Failed to create pipeline: {:?}", e);
                    Err(e)
                }
            }
        });

        //will block until we get the sdp answer or all tx are dropped
        let Ok(sdp_answer) = rx.recv() else {
            return Err(anyhow!("Failed to get sdp answer"));
        };

        Ok(WebrtcPipeline { mainloop, pipeline_thread: Some(pipeline_thread), sdp_answer })
    }

    pub fn get_sdp_answer(&self) -> String {
        self.sdp_answer.clone()
    }
}

impl Drop for WebrtcPipeline {
    fn drop(&mut self) {
        info!("Dropping WebrtcPipeline");
        self.mainloop.quit();
        if let Some(handle) = self.pipeline_thread.take() {
            if let Err(e) = handle.join() {
                error!("Failed to join pipeline thread: {:?}", e);
            }
        }
    }
}

//create the gstreamer pipeline
fn create_pipeline(
    main_loop: glib::MainLoop, vdevice: String, sdp_offer: String, tx: mpsc::Sender<String>,
    video_prop: VideoProp,
) -> Result<()> {
    gst::init()?;

    let pipeline = Pipeline::default();

    let webrtcbin = ElementFactory::make("webrtcbin").build()?;

    webrtcbin.set_property("latency", 0u32);
    webrtcbin.set_property("bundle-policy", WebRTCBundlePolicy::None);

    let decodebin = ElementFactory::make("decodebin").build()?;

    //use the max-bundle policy which means that all media streams will be multiplexed into a
    //single transport

    let queue = ElementFactory::make("queue").build()?;

    let rtph264depay = ElementFactory::make("rtph264depay").build()?;
    let h264dec = ElementFactory::make("avdec_h264").build()?;
    let h264parse = ElementFactory::make("h264parse").build()?;
    let videosink = ElementFactory::make("autovideosink").build()?;

    let videoconvert = ElementFactory::make("videoconvert").build()?;
    let videoconvert2 = ElementFactory::make("videoconvert").build()?;
    let videoscale = ElementFactory::make("videoscale").build()?;
    let videoscale2 = ElementFactory::make("videoscale").build()?;
    let videorate = ElementFactory::make("videorate").build()?;

    videorate.set_property("max-rate", video_prop.fps as i32);

    //setting video properties
    let capsfilter = ElementFactory::make("capsfilter").build()?;
    let caps = gst::Caps::builder("video/x-raw")
        .field("width", 1080)
        .field("height", 720)
        .field("format", "I420")
        .build();

    capsfilter.set_property("caps", &caps);

    //    let v4l2sink = ElementFactory::make("v4l2sink").build()?;

    /*
     * NV12 @ 540x960
     */
    //configure the virtual device
    let v4l_dev = Device::with_path(&vdevice)
        .map_err(|e| anyhow!("Failed to create v4l2 device: {:?}", e))?;

    //set NV12 format and resolution 540x960
    let mut format =
        v4l_dev.format().map_err(|e| anyhow!("Failed to get v4l2 device format: {:?}", e))?;
    info!("v4l2 format: {:?}", format);

    format.fourcc = FourCC::new(b"NV12");
    format.width = 540;
    format.height = 960;

    v4l_dev
        .set_format(&format)
        .map_err(|e| anyhow!("Failed to set v4l2 device format: {:?}", e))?;

    //read the format again
    let format =
        v4l_dev.format().map_err(|e| anyhow!("Failed to get v4l2 device format: {:?}", e))?;

    info!("v4l2 format after configured: {:?}", format);

    //v4l2sink.set_property("device", &vdevice);

    let appsink = ElementFactory::make("appsink").build()?;
    appsink.set_property("emit-signals", true);
    appsink.set_property("sync", false);

    //set the caps for the appsink
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", "I420")
        .field("width", 540)
        .field("height", 960)
        .field("framerate", Fraction::new(video_prop.fps as i32, 1))
        .build();

    //appsink.set_property("caps", &caps);

    appsink.connect("new-sample", false, move |values| {
        let appsink = values[0].get::<gst_app::AppSink>().unwrap();
        let sample = appsink.pull_sample().unwrap();

        info!("Received new sample from appsink");
        let buffer = sample.buffer().unwrap();

        // Map the buffer to read its data
        let map = buffer.map_readable().unwrap();
        let data = map.as_slice();

        // Open the v4l2loopback device
        let mut device = OpenOptions::new()
            .write(true)
            .open(&vdevice) // Replace with your v4l2loopback device path
            .unwrap();

        // Write the buffer data to the device
        device.write_all(data).unwrap();

        info!("Buffer size: {}", buffer.size());
        //write the frame to the v4l2loopback device

        Some(FlowReturn::Ok.to_value())
    });

    pipeline.add_many(&[
        &webrtcbin,
        &decodebin,
        &queue,
        //&rtph264depay,
        //&h264parse,
        //&h264dec,
        &videoconvert,
        &videoscale,
        //&capsfilter,
        //&videoconvert2,
        //&videoscale2,
        //&v4l2sink,
        //&appsink,
        &videosink,
    ])?;

    gst::Element::link_many(&[
        &queue,
        // &rtph264depay,
        // &h264parse,
        // &h264dec,
        &videoconvert,
        &videoscale,
        //&capsfilter,
        // &videoconvert2,
        // &videoscale2,
        //&v4l2sink,
        //&appsink,
        &videosink,
    ])?;

    //configure decodebin
    let queue_clone = queue.clone();

    decodebin.connect("pad-added", false, move |values| {
        let _decodebin = values[0].get::<gst::Element>().unwrap();
        let pad = values[1].get::<gst::Pad>().unwrap();

        let caps = pad.current_caps().unwrap();
        let name = caps.structure(0).unwrap().name();

        if name.starts_with("video/") {
            let sink_pad = queue_clone.static_pad("sink").unwrap();

            if sink_pad.is_linked() {
                info!("Decodebin pad is already linked to queue");
                return None;
            }

            match pad.link(&sink_pad) {
                Ok(_) => {
                    info!("Linked decodebin to queue successfully.");
                }
                Err(err) => {
                    info!("Failed to link decodebin: {:?}", err);
                }
            }
        }

        None
    });

    let decodebin_clone = decodebin.clone();

    webrtcbin.connect("pad-added", false, move |values| {
        info!("Pad added signal received");
        let Ok(_webrtc) = values[0].get::<gst::Element>() else {
            error!("Expected webrtcbin element");
            return None;
        };

        let Ok(new_pad) = values[1].get::<gst::Pad>() else {
            error!("Expected pad from webrtcbin");
            return None;
        };

        let Some(caps) = new_pad.current_caps().or_else(|| new_pad.allowed_caps()) else {
            error!("Failed to get caps from new pad");
            return None;
        };

        let Some(s) = caps.structure(0) else {
            error!("Failed to get caps structure");
            return None;
        };

        let media_type = s.name();

        if media_type.starts_with("application/x-rtp") {
            let Some(sink_pad) = decodebin_clone.static_pad("sink") else {
                error!("Failed to get queue sink pad");
                return None;
            };

            if sink_pad.is_linked() {
                info!("Webrtcbin pad is already linked to decodebin");
                return None;
            }

            match new_pad.link(&sink_pad) {
                Ok(_) => {
                    info!("Linked webrtcbin pad to decodebin successfully.");
                }
                Err(err) => {
                    info!("Failed to link webrtcbin pad: {:?}", err);
                }
            }
        }
        None
    });

    webrtcbin.connect("on-negotiation-needed", false, move |_values| {
        info!("Negotiation needed signal received (waiting for an external offer)...");
        None
    });

    webrtcbin.connect("on-ice-candidate", false, move |values| {
        let Ok(_) = values[0].get::<gst::Element>() else {
            error!("Expected webrtcbin element");
            return None;
        };

        let Ok(mlineindex) = values[1].get::<u32>() else {
            error!("Expected mline index");
            return None;
        };

        let Ok(candidate) = values[2].get::<String>() else {
            error!("Expected candidate string");
            return None;
        };

        info!("New ICE candidate gathered (mline index {}): {}", mlineindex, candidate);
        None
    });

    let webrtcbin_clone = webrtcbin.clone();
    let tx_clone = tx.clone();

    webrtcbin.connect_notify(Some("ice-gathering-state"), move |webrtc, _pspec| {
        info!("ICE gathering state changed");
        let webrtcbin_clone = webrtcbin_clone.clone();
        let tx_clone = tx_clone.clone();
        let state = webrtc.property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state");

        info!("ICE gathering state changed: {:?}", state);
        if state == gst_webrtc::WebRTCICEGatheringState::Complete {
            let Ok(sdp_answer) = webrtcbin_clone
                .property::<gst_webrtc::WebRTCSessionDescription>("local-description")
                .sdp()
                .as_text()
            else {
                error!("Failed to get SDP answer");
                return;
            };

            debug!("Sending SDP answer to main thread {}", sdp_answer);
            let Ok(_) = tx_clone.send(sdp_answer) else {
                error!("Failed to send SDP answer to main thread");
                return;
            };
        }
    });

    // bus error handling
    let bus = pipeline.bus().ok_or(anyhow!("Failed to get bus"))?;

    let main_loop_clone = main_loop.clone();

    let _bus_watch = bus.add_watch(move |_, msg| {
        use gst::MessageView;

        let main_loop = &main_loop_clone;
        match msg.view() {
            MessageView::Eos(..) => {
                info!("received eos");
                // An EndOfStream event was sent to the pipeline, so we tell our main loop
                // to stop execution here.
                main_loop.quit()
            }
            MessageView::Error(err) => {
                error!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                //main_loop.quit()
            }
            _ => (),
        };

        // Tell the mainloop to continue executing this callback.
        glib::ControlFlow::Continue
    })?;

    pipeline.set_state(gst::State::Playing)?;

    let sdp = gst_sdp::SDPMessage::parse_buffer(sdp_offer.as_bytes())?;

    info!("Parsed SDP offer:\n{}", sdp);

    let offer = gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, sdp);

    let webrtcbin_clone = webrtcbin.clone();
    let promise_offer = gst::Promise::with_change_func(move |reply| {
        let _reply = match reply {
            Ok(_) => {
                info!("successfully created offer");
            }
            Err(err) => {
                error!("Offer creation future got error response: {:?}", err);
                return;
            }
        };

        let offer_desc =
            webrtcbin_clone.property::<gst_webrtc::WebRTCSessionDescription>("remote-description");

        info!("Remote description set: {:?}", offer_desc.sdp().as_text());

        let webrtcbin_clone2 = webrtcbin_clone.clone();

        let promise = gst::Promise::with_change_func(move |reply| {
            let reply = match reply {
                Ok(Some(reply)) => Some(reply),
                Ok(None) => {
                    error!("Answer creation future got no response");
                    None
                }
                Err(err) => {
                    error!("Answer creation future got error response: {:?}", err);
                    None
                }
            };

            let Some(reply) = reply else {
                error!("Failed to get reply from answer creation future");
                return;
            };

            let Ok(answer) = reply.get::<gst_webrtc::WebRTCSessionDescription>("answer") else {
                error!("Failed to get SDP answer from reply");
                return;
            };

            let Ok(sdp_answer) = answer.sdp().as_text() else {
                error!("Failed to get SDP text from answer");
                return;
            };

            debug!("Created SDP answer:\n{}", sdp_answer);

            webrtcbin_clone
                .emit_by_name::<()>("set-local-description", &[&answer, &None::<gst::Promise>]);
        });

        webrtcbin_clone2.emit_by_name::<()>("create-answer", &[&None::<gst::Structure>, &promise]);
    });

    webrtcbin.emit_by_name::<()>("set-remote-description", &[&offer, &promise_offer]);

    // Start the main loop in a separate thread
    info!("Starting main loop");

    main_loop.run();

    info!("Main loop stopped");

    pipeline.set_state(gst::State::Null)?;

    Ok(())
}
