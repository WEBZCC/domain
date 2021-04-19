//! EDNS Options from RFC 7871

use super::super::iana::OptionCode;
use super::super::message_builder::OptBuilder;
use super::super::net::IpAddr;
use super::super::octets::{
    Compose, FormError, OctetsBuilder, Parse, ParseError, Parser, ShortBuf
};
use super::CodeOptData;


//------------ ClientSubnet --------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClientSubnet {
    source_prefix_len: u8,
    scope_prefix_len: u8,
    addr: IpAddr,
}

impl ClientSubnet {
    pub fn new(
        source_prefix_len: u8,
        scope_prefix_len: u8,
        addr: IpAddr
    ) -> ClientSubnet {
        ClientSubnet { source_prefix_len, scope_prefix_len, addr }
    }

    pub fn push<Target: OctetsBuilder>(
        builder: &mut OptBuilder<Target>,
        source_prefix_len: u8,
        scope_prefix_len: u8,
        addr: IpAddr
    ) -> Result<(), ShortBuf> {
        builder.push(&Self::new(source_prefix_len, scope_prefix_len, addr))
    }

    pub fn source_prefix_len(&self) -> u8 { self.source_prefix_len }
    pub fn scope_prefix_len(&self) -> u8 { self.scope_prefix_len }
    pub fn addr(&self) -> IpAddr { self.addr }
}


//--- Parse and Compose

impl<Ref: AsRef<[u8]>> Parse<Ref> for ClientSubnet {
    fn parse(parser: &mut Parser<Ref>) -> Result<Self, ParseError> {
        let family = parser.parse_u16()?;
        let source_prefix_len = parser.parse_u8()?;
        let scope_prefix_len = parser.parse_u8()?;

        // https://tools.ietf.org/html/rfc7871#section-6
        //
        // | ADDRESS, variable number of octets, contains either an IPv4 or
        // | IPv6 address, depending on FAMILY, which MUST be truncated to
        // | the number of bits indicated by the SOURCE PREFIX-LENGTH field,
        // | padding with 0 bits to pad to the end of the last octet needed.
        let (prefix_bytes, mask) =
            prefix_bytes(usize::from(source_prefix_len));

        let addr = match family {
            1 => {
                let mut buf = [0; 4];
                if prefix_bytes > buf.len() {
                    return Err(
                        FormError::new(
                            "invalid address length in client subnet option"
                        ).into()
                    );
                }
                parser.parse_buf(&mut buf[..prefix_bytes])?;
                if let Some(e) = buf.get_mut(prefix_bytes - 1) {
                    *e &= mask;
                }
                IpAddr::from(buf)
            }
            2 => {
                let mut buf = [0; 16];
                if prefix_bytes > buf.len() {
                    return Err(
                        FormError::new(
                            "invalid address length in client subnet option"
                        ).into()
                    );
                }
                parser.parse_buf(&mut buf[..prefix_bytes])?;
                if let Some(e) = buf.get_mut(prefix_bytes - 1) {
                    *e &= mask;
                }
                IpAddr::from(buf)
            }
            _ => {
                return Err(
                    FormError::new(
                        "invalid client subnet address family"
                    ).into()
                )
            }
        };
        Ok(ClientSubnet::new(source_prefix_len, scope_prefix_len, addr))
    }

    fn skip(parser: &mut Parser<Ref>) -> Result<(), ParseError> {
        // XXX Perhaps do a check?
        parser.advance_to_end();
        Ok(())
    }
}

impl Compose for ClientSubnet {
    fn compose<T: OctetsBuilder>(
        &self,
        target: &mut T
    ) -> Result<(), ShortBuf> {
        let (prefix_bytes, mask) =
            prefix_bytes(self.source_prefix_len as usize);
        target.append_all(|target| match self.addr {
            IpAddr::V4(addr) => {
                1u16.compose(target)?;
                self.source_prefix_len.compose(target)?;
                self.scope_prefix_len.compose(target)?;
                let mut array = addr.octets();
                if let Some(e) = array.get_mut(prefix_bytes - 1) {
                    *e &= mask;
                }
                target.append_slice(&array[..prefix_bytes])
            }
            IpAddr::V6(addr) => {
                2u16.compose(target)?;
                self.source_prefix_len.compose(target)?;
                self.scope_prefix_len.compose(target)?;
                let mut array = addr.octets();
                if let Some(e) = array.get_mut(prefix_bytes - 1) {
                    *e &= mask;
                }
                target.append_slice(&array[..prefix_bytes])
            }
        })
    }
}

fn prefix_bytes(bits: usize) -> (usize, u8) {
    let n = (bits + 7) / 8;
    let mask = match 8 - (bits % 8) {
        0 => 0xff,
        n => 0xff << n,
    };
    (n, mask)
}

//--- CodeOptData

impl CodeOptData for ClientSubnet {
    const CODE: OptionCode = OptionCode::ClientSubnet;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::octets::Octets512;
    use core::convert::TryFrom;

    #[test]
    fn truncate_bits() {
        // no extra truncation
        let csub = ClientSubnet::new(23, 0, "192.0.2.0".parse().unwrap());
        let mut buf = Octets512::new();
        csub.compose(&mut buf).unwrap();
        assert_eq!(buf.as_ref(), [0, 1, 23, 0, 192, 0, 2].as_ref());

        let parsed =
            ClientSubnet::parse(&mut Parser::from_ref(buf.as_ref())).unwrap();
        assert_eq!(parsed, csub);

        // with extra truncation
        let opt_bytes = [0, 1, 22, 0, 192, 0, 2];
        let csub = ClientSubnet::parse(&mut Parser::from_ref(
            Octets512::try_from(opt_bytes.as_ref()).unwrap(),
        ))
        .unwrap();
        assert_eq!(csub.addr(), "192.0.0.0".parse::<IpAddr>().unwrap());

        let csub = ClientSubnet::new(22, 0, "192.0.2.0".parse().unwrap());
        let mut buf = Octets512::new();
        csub.compose(&mut buf).unwrap();
        assert_eq!(buf.as_ref(), [0, 1, 22, 0, 192, 0, 0].as_ref());
    }
}
