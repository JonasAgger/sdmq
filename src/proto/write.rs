use crate::proto::{MAGIC, SdmqHeader, SdmqMsgType};

pub type SdmqPacket = SdmqPacketBuf<1024>;

pub struct SdmqPacketBuf<const SIZE: usize> {
    buffer: [u8; SIZE],
    pos: usize,
}

impl<const SIZE: usize> Default for SdmqPacketBuf<SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const MAX_SIZE: usize> SdmqPacketBuf<MAX_SIZE> {
    pub fn new() -> Self {
        let buffer = [0u8; MAX_SIZE];

        Self { buffer, pos: 0 }
    }

    #[cfg(any(feature = "serde_json_core", feature = "serde_json"))]
    pub fn write_json<S, D>(&mut self, topic: S, data: D)
    where
        S: AsRef<str>,
        D: serde::Serialize,
    {
        // Write topic
        let topic = topic.as_ref();

        let cursor = SdMessageBuilder::new(&mut self.buffer);

        let pos = cursor.topic(topic).write_json(data).done(SdmqMsgType::Push);

        self.pos = pos;
    }

    pub fn write_raw<S>(&mut self, topic: &S, data: &[u8])
    where
        S: AsRef<str>,
    {
        // Write topic
        let topic = topic.as_ref();

        let cursor = SdMessageBuilder::new(&mut self.buffer);

        let pos = cursor.topic(topic).write_raw(data).done(SdmqMsgType::Push);

        self.pos = pos;
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buffer[..self.pos]
    }
}

impl<const MAX_SIZE: usize> AsRef<[u8]> for SdmqPacketBuf<MAX_SIZE> {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

pub struct Empty;
pub struct Topic(usize);
pub struct Data(usize);
#[cfg(feature = "std")]
struct Intermediate(usize);

pub struct SdMessageBuilder<'a, T, D> {
    inner: &'a mut [u8],
    topic: T,
    data: D,
}

impl<'a> SdMessageBuilder<'a, Empty, Empty> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self {
            inner: buf,
            topic: Empty,
            data: Empty,
        }
    }

    pub fn topic(self, data: &str) -> SdMessageBuilder<'a, Topic, Empty> {
        let offset = size_of::<SdmqHeader>();

        let bytes = data.as_bytes();

        let topic_len = bytes.len();
        let buf = &mut self.inner[offset..];
        buf[..topic_len].copy_from_slice(bytes);

        SdMessageBuilder {
            inner: self.inner,
            topic: Topic(topic_len),
            data: self.data,
        }
    }
}

impl<'a> SdMessageBuilder<'a, Topic, Empty> {
    #[cfg(all(feature = "serde_json_core", not(feature = "serde_json")))]
    pub fn write_json(self, data: impl serde::Serialize) -> SdMessageBuilder<'a, Topic, Data> {
        let offset = size_of::<SdmqHeader>() + self.topic.0;
        let written = serde_json_core::to_slice(&data, &mut self.inner[offset..]).unwrap();

        SdMessageBuilder {
            inner: self.inner,
            topic: self.topic,
            data: Data(written),
        }
    }

    #[cfg(feature = "serde_json")]
    pub fn write_json(self, data: impl serde::Serialize) -> SdMessageBuilder<'a, Topic, Data> {
        let mut s = SdMessageBuilder {
            inner: self.inner,
            topic: self.topic,
            data: Intermediate(0),
        };
        serde_json::to_writer(&mut s, &data).unwrap();

        SdMessageBuilder {
            inner: s.inner,
            topic: s.topic,
            data: Data(s.data.0),
        }
    }

    #[cfg(feature = "std")]
    pub fn as_write_fn<F>(self, write_fn: F) -> SdMessageBuilder<'a, Topic, Data>
    where
        F: FnOnce(&mut dyn std::io::Write),
    {
        let mut s = SdMessageBuilder {
            inner: self.inner,
            topic: self.topic,
            data: Intermediate(0),
        };

        write_fn(&mut s);

        SdMessageBuilder {
            inner: s.inner,
            topic: s.topic,
            data: Data(s.data.0),
        }
    }

    pub fn write_raw(self, data: &[u8]) -> SdMessageBuilder<'a, Topic, Data> {
        let offset = size_of::<SdmqHeader>() + self.topic.0;

        let buf = &mut self.inner[offset..];

        buf[..data.len()].copy_from_slice(data);

        SdMessageBuilder {
            inner: self.inner,
            topic: self.topic,
            data: Data(data.len()),
        }
    }
}

impl<'a> SdMessageBuilder<'a, Topic, Data> {
    pub fn done(self, msg_type: SdmqMsgType) -> usize {
        let pos = self.data.0 + self.topic.0 + size_of::<SdmqHeader>();

        // CRC_hash topic + payload
        let hash = self.crc32_ieee();

        let header = SdmqHeader {
            magic: MAGIC,
            hash,
            options: msg_type as u16,
            topic_len: self.topic.0 as u16,
            data_len: self.data.0 as u32,
        };

        header.write_to(self.inner);

        pos
    }

    /// Bitwise CRC-32 (IEEE) with init=0xFFFF_FFFF and xorout=0xFFFF_FFFF.
    /// This matches the common "PKZip/Ethernet" CRC-32.
    fn crc32_ieee(&self) -> u32 {
        let pos = self.data.0 + self.topic.0 + size_of::<SdmqHeader>();
        let mut crc: u32 = 0xFFFF_FFFF;
        let data = &self.inner[size_of::<SdmqHeader>()..pos];

        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                let mask = 0u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }

        !crc
    }
}

#[cfg(feature = "std")]
impl<'a> std::io::Write for SdMessageBuilder<'a, Topic, Intermediate> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let data_start = self.topic.0 + size_of::<SdmqHeader>();
        let pos = data_start + self.data.0;

        let self_buf = &mut self.inner[pos..];

        self_buf[..buf.len()].copy_from_slice(buf);

        self.data = Intermediate(self.data.0 + buf.len());
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
