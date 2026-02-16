use crate::{
    proto::{SdmqHeader, SdmqMsgType},
    topic::Topic,
};

pub struct SdmqProto<'a> {
    pub(super) header: SdmqHeader,
    pub(super) topic: Topic<'a>,
    pub(super) data: &'a [u8],
}

impl<'a> SdmqProto<'a> {
    pub fn parse(buf: &'a [u8]) -> Self {
        let header = SdmqHeader::parse(buf);

        let total_len = header.topic_len as usize + header.data_len as usize;
        let end = total_len + size_of::<SdmqHeader>();
        let clamped = end.min(buf.len());

        let (topic, data) =
            buf[size_of::<SdmqHeader>()..clamped].split_at(header.topic_len as usize);

        let topic = Topic::parse(topic);

        Self {
            header,
            topic,
            data,
        }
    }

    pub fn msg_type(&self) -> SdmqMsgType {
        match self.header.options & 0x0000_1111 {
            1 => SdmqMsgType::Push,
            2 => SdmqMsgType::Sub,
            other => panic!("Received unknown msg type: {}", other),
        }
    }

    pub fn topic(&self) -> Topic<'_> {
        self.topic
    }

    pub fn data(&self) -> &[u8] {
        self.data
    }

    #[cfg(feature = "std")]
    pub fn data_to_vec(&self) -> Vec<u8> {
        self.data.to_vec()
    }
}
