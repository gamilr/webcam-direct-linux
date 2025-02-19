use crate::{ble::mobile_sdp_types::VideoProp, error::Result};
use anyhow::anyhow;
use std::{sync::mpsc, thread, time::Duration};

use gst::{
    glib::{self, MainLoop},
    prelude::*,
    ElementFactory, Pipeline,
};


use gstreamer::{self as gst, Fraction};
use gstreamer_webrtc::gst_sdp;
use gstreamer_webrtc::{self as gst_webrtc};
use log::{error, debug, info};

#[derive(Debug)]
pub struct WebrtcPipeline {
    mainloop: MainLoop,
    pipeline_thread: Option<thread::JoinHandle<Result<()>>>,
    sdp_answer: String,
}

impl WebrtcPipeline {
    pub fn new(
        vdevice: String, sdp_offer: String, video_prop: VideoProp,
    ) -> Result<Self> {
        let mainloop = glib::MainLoop::new(None, false);

        let (tx, rx) = mpsc::channel();

        let mainloop_clone = mainloop.clone();

        let pipeline_thread = thread::spawn(move || {
            match create_pipeline(
                mainloop_clone,
                vdevice,
                sdp_offer,
                tx,
                video_prop,
            ) {
                Ok(_) => Ok(()),
                Err(e) => {
                    error!("Failed to create pipeline: {:?}", e);
                    Err(e)
                }
            }
        });

        //will block until we get the sdp answer or all tx are dropped
        let Ok(sdp_answer) = rx.recv_timeout(Duration::from_secs(3)) else {
            return Err(anyhow!("Failed to get sdp answer"));
        };

        Ok(WebrtcPipeline {
            mainloop,
            pipeline_thread: Some(pipeline_thread),
            sdp_answer,
        })
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
    main_loop: glib::MainLoop, vdevice: String, sdp_offer: String,
    tx: mpsc::Sender<String>, video_prop: VideoProp,
) -> Result<()> {
    gst::init()?;

    let pipeline = Pipeline::default();

    let webrtcbin = ElementFactory::make("webrtcbin").build()?;

    //use the max-bundle policy which means that all media streams will be multiplexed into a
    //single transport
    webrtcbin.set_property_from_str("bundle-policy", "max-bundle");
    //gather all ice candidates before creating the answer

    let queue = ElementFactory::make("queue").build()?;

    let rtpvp8depay = ElementFactory::make("rtpvp8depay").build()?;
    let vp8dec = ElementFactory::make("vp8dec").build()?;
    let videoconvert = ElementFactory::make("videoconvert").build()?;

    //setting video properties
    let capsfilter = ElementFactory::make("capsfilter").build()?;
    let caps = gst::Caps::builder("video/x-raw")
        .field("width", video_prop.resolution.0)
        .field("height", video_prop.resolution.1)
        .field("framerate", Fraction::new(video_prop.fps as i32, 1))
        .build();

    capsfilter.set_property("caps", &caps);

    let v4l2sink = ElementFactory::make("v4l2sink").build()?;

    v4l2sink.set_property("device", &vdevice);

    pipeline.add_many(&[
        &webrtcbin,
        &queue,
        &rtpvp8depay,
        &vp8dec,
        &videoconvert,
        //&capsfilter,
        &v4l2sink,
    ])?;

    gst::Element::link_many(&[
        &queue,
        &rtpvp8depay,
        &vp8dec,
        &videoconvert,
        //&capsfilter,
        &v4l2sink,
    ])?;

    let queue_clone = queue.clone();

    webrtcbin.connect("pad-added", false, move |values| {
        let Ok(_webrtc) = values[0]
            .get::<gst::Element>() else {
            error!("Expected webrtcbin element");
            return None;
        };

        let Ok(new_pad) = values[1]
            .get::<gst::Pad>() else {
            error!("Expected pad from webrtcbin");
            return None;
            };

        let Some(caps) = new_pad
            .current_caps()
            .or_else(|| new_pad.allowed_caps()) else {
            error!("Failed to get caps from new pad");
            return None;
            };

        let Some(s) = caps.structure(0) else {
            error!("Failed to get caps structure");
            return None;
        };

        let media_type = s.name();

        if media_type.starts_with("application/x-rtp") {
            let Some(sink_pad) = queue_clone
                .static_pad("sink") else {
                error!("Failed to get queue sink pad");
                return None;
                };

            if sink_pad.is_linked() {
                info!("Webrtcbin pad is already linked to queue");
                return None;
            }

            match new_pad.link(&sink_pad) {
                Ok(_) => {
                    info!("Linked webrtcbin pad to queue successfully.");
                }
                Err(err) => {
                    info!("Failed to link webrtcbin pad: {:?}", err);
                }
            }
        }
        None
    });

    webrtcbin
            .connect("on-negotiation-needed", false, move |_values| {
                info!("Negotiation needed signal received (waiting for an external offer)...");
                None
            });

    
    webrtcbin.connect("on-ice-candidate", false, move |values| {
        let Ok(_) = values[0]
            .get::<gst::Element>() else {
            error!("Expected webrtcbin element");
            return None;
            };

        let Ok(mlineindex) = values[1].get::<u32>() else {
            error!("Expected mline index");
            return None;
        };

        let Ok(candidate) =
            values[2].get::<String>() else {
            error!("Expected candidate string");
            return None;
            };

        info!(
            "New ICE candidate gathered (mline index {}): {}",
            mlineindex, candidate
        );
        None
    });

    pipeline.set_state(gst::State::Playing)?;

    // bus error handling
    let bus = pipeline.bus().ok_or(anyhow!("Failed to get bus"))?;

    let main_loop_clone = main_loop.clone();

    let _bus_watch = bus
        .add_watch(move |_, msg| {
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
                }
                _ => (),
            };

            // Tell the mainloop to continue executing this callback.
            glib::ControlFlow::Continue
        })?;


    /*
        let sdp_offer = "v=0\r\no=- 4611733054762223410 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=group:BUNDLE 0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\nc=IN IP4 0.0.0.0\r\na=mid:0\r\na=sendonly\r\na=rtcp-mux\r\na=rtpmap:96 VP8/90000\r\n";
    */

    info!("Received SDP offer:\n{}", sdp_offer);
    let sdp = gst_sdp::SDPMessage::parse_buffer(sdp_offer.as_bytes())?;

    let offer = gst_webrtc::WebRTCSessionDescription::new(
        gst_webrtc::WebRTCSDPType::Offer,
        sdp,
    );

    webrtcbin.emit_by_name::<()>("set-remote-description", &[&offer, &None::<gst::Promise>]);

    let webrtcbin_clone = webrtcbin.clone();
    let tx_clone = tx.clone();
    let main_loop_clone = main_loop.clone();

    webrtcbin.connect_notify(
        Some("ice-gathering-state"),
        move |webrtc, _pspec| {
            let webrtcbin_clone = webrtcbin_clone.clone();
            let tx_clone = tx_clone.clone();
            let state = webrtc.property::<gst_webrtc::WebRTCICEGatheringState>(
                "ice-gathering-state",
            );

            let main_loop_clone = main_loop_clone.clone();

            info!("ICE gathering state changed: {:?}", state);
            if state == gst_webrtc::WebRTCICEGatheringState::Complete {

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
                        main_loop_clone.quit();
                        return;
                    };


                    let Ok(answer) = reply
                        .get::<gst_webrtc::WebRTCSessionDescription>("answer") else {
                        error!("Failed to get SDP answer from reply");
                        main_loop_clone.quit();
                        return;
                    };

                    let Ok(sdp_answer) = answer.sdp().as_text() else {
                        error!("Failed to get SDP text from answer");
                        main_loop_clone.quit();
                        return;
                    };

                    debug!(
                        "Created SDP answer:\n{}", sdp_answer
                        
                    );

                    webrtcbin_clone.emit_by_name::<()>(
                        "set-local-description",
                        &[&answer],
                    );

                    debug!("Sending SDP answer to main thread");

                    let Ok(_) = tx_clone.send(sdp_answer) else {
                        error!("Failed to send SDP answer to main thread");
                        main_loop_clone.quit();
                        return;
                    };

                });

                webrtc.emit_by_name::<()>("create-answer", &[&promise]);
            }
        },
    );

    main_loop.run();

    pipeline
        .set_state(gst::State::Null)?;

    Ok(())
}
