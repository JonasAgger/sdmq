pub(crate) struct Cursor<T> {
    inner: T,
    pos: usize,
}

impl<T> Cursor<T> {
    pub fn new(buf: T) -> Self {
        Self { inner: buf, pos: 0 }
    }
}

impl<T: AsRef<[u8]>> Cursor<T> {
    pub fn read_u16(&mut self) -> u16 {
        const SIZE: usize = size_of::<u16>();

        let buf = &self.inner.as_ref()[self.pos..];

        let res = u16::from_be_bytes(buf[..SIZE].try_into().unwrap());

        self.pos += SIZE;
        res
    }

    pub fn read_u32(&mut self) -> u32 {
        const SIZE: usize = size_of::<u32>();

        let buf = &self.inner.as_ref()[self.pos..];

        let res = u32::from_be_bytes(buf[..SIZE].try_into().unwrap());

        self.pos += SIZE;
        res
    }
}

impl<T: AsMut<[u8]>> Cursor<T> {
    pub fn write_n<const N: usize>(&mut self, data: [u8; N]) {
        let buf = &mut self.inner.as_mut()[self.pos..];

        buf[..N].copy_from_slice(&data);

        self.pos += N;
    }
}
