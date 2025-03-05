//! This module provides functionality for parsing nl80211 payloads.
//! It defines structures and functions to extract wireless device properties
//! from netlink messages received from the nl80211 subsystem.

use super::nl80211_const::Nl80211Iftype;
use crate::error::Result;

use log::{info, trace};
use neli::{
    attr::Attribute,
    genl::{Genlmsghdr, Nlattr},
};

use super::InterfaceIndex;

use super::nl80211_const::{Nl80211Attribute, Nl80211Command};

#[derive(Default, Debug)]
pub struct WiPhyProps {
    pub phy_idx: Option<InterfaceIndex>,
    pub ap_supported: Option<bool>,
    pub if_idx: Option<InterfaceIndex>,
}

/// Parses the nl80211 payload from a generic netlink message and extracts
/// wireless device properties.
///
/// # Arguments
///
/// * `gen_msg` - A reference to the generic netlink message header containing
///               the nl80211 payload.
///
/// # Returns
///
/// A `Result` containing `WiPhyProps` with the extracted properties.
pub fn parse_nl80211_payload(
    gen_msg: &Genlmsghdr<Nl80211Command, Nl80211Attribute>,
) -> Result<WiPhyProps> {
    trace!("Received message {:#?}", gen_msg);

    let mut props =
        WiPhyProps { phy_idx: None, ap_supported: None, if_idx: None };

    let attr_handle = gen_msg.get_attr_handle();
    for attr in attr_handle.iter() {
        match attr.nla_type.nla_type {
            //get index
            Nl80211Attribute::Wiphy => {
                props.phy_idx =
                    Some(InterfaceIndex(attr.get_payload_as::<u16>()?));
                info!("Phy index: {:?}", props.phy_idx);
            }

            //get interface index
            Nl80211Attribute::Ifindex => {
                props.if_idx =
                    Some(InterfaceIndex(attr.get_payload_as::<u16>()?));
                info!("Interface index: {:?}", props.if_idx);
            }

            //get software interface types
            Nl80211Attribute::SoftwareIftypes => {
                //parse the nested attributes
                let attr_vec: Vec<_> = attr.get_payload_as_with_len::<Vec<Nlattr<Nl80211Iftype, &[u8]>>>()?;

                //check if any AP modes is supported
                props.ap_supported = attr_vec
                    .iter()
                    .find(|x| {
                        x.nla_type.nla_type == Nl80211Iftype::IftypeApVlan
                            || x.nla_type.nla_type == Nl80211Iftype::IftypeAp
                    })
                    .map(|_| true);

                info!("AP mode supported: {:?}", props.ap_supported);
            }
            _ => (),
        }
    }

    Ok(props)
}
