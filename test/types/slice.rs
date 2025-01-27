#![feature(lang_items,adt_const_params,associated_type_defaults,core_intrinsics,start,ascii_char,slice_internals)]
#![allow(internal_features,incomplete_features,unused_variables,dead_code)]
#![no_std]
include!("../common.rs");
fn main(){
    let ptr = unsafe{malloc(64) as *mut _};
    black_box(ptr);
    let slice:&mut [u8] = unsafe{core::slice::from_raw_parts_mut(ptr,64)};
    let len = slice.len();
    let first = slice[black_box(0)];
    Put::putnl(len as u64);
    Put::putnl(first);
    slice[black_box(0)] = 'H' as u8;
    slice[black_box(1)] = 'e' as u8;
    slice[black_box(2)] = 'l' as u8;
    slice[black_box(3)] = 'l' as u8;
    slice[black_box(4)] = 'o' as u8;
    slice[black_box(5)] = '.' as u8;
    slice[black_box(6)] = '\n' as u8;
    slice[black_box(7)] = 0;
    unsafe{puts(ptr)};
    black_box(&slice);
    let oslice = b"Hello, World\n\0";
    test_eq!(oslice.len(),14);
    test_eq!(oslice.last(),Some(&b'\0'));
    if let Some((first,rem)) = oslice.split_first(){
        unsafe{printf(rem.as_ptr() as *const i8)};
        test_eq!(rem.len(),oslice.len() - 1);
        unsafe{printf("%c\n\0".as_ptr() as *const i8,*first  as i32)};
    }
    test_eq!(oslice.split_first(),Some((&b'H',b"ello, World\n\0")));
    test_eq!(memrchr(b'W',b"Hello, World\n\0"),Some(7));
    dump_var(0,0,true,1,1,2,2,3,false);
    
}
#[must_use]
pub fn memrchr(x: u8, text: &[u8]) -> Option<usize> {
    // Scan for a single byte value by reading two `usize` words at a time.
    //
    // Split `text` in three parts:
    // - unaligned tail, after the last word aligned address in text,
    // - body, scanned by 2 words at a time,
    // - the first remaining bytes, < 2 word size.
    let len = text.len();
    let ptr = text.as_ptr();
    type Chunk = usize;

    let (min_aligned_offset, max_aligned_offset) = {
        // We call this just to obtain the length of the prefix and suffix.
        // In the middle we always process two chunks at once.
        // SAFETY: transmuting `[u8]` to `[usize]` is safe except for size differences
        // which are handled by `align_to`.
        let (prefix, _, suffix) = unsafe { text.align_to::<(Chunk, Chunk)>() };
        (prefix.len(), len - suffix.len())
    };

    let mut offset = max_aligned_offset;
    if let Some(index) = text[offset..].iter().rposition(|elt| *elt == x) {
        return Some(offset + index);
    }

    // Search the body of the text, make sure we don't cross min_aligned_offset.
    // offset is always aligned, so just testing `>` is sufficient and avoids possible
    // overflow.
    let repeated_x = repeat_u8(x);
    let chunk_bytes = core::mem::size_of::<Chunk>();

    while offset > min_aligned_offset {
        // SAFETY: offset starts at len - suffix.len(), as long as it is greater than
        // min_aligned_offset (prefix.len()) the remaining distance is at least 2 * chunk_bytes.
        unsafe {
            let u = *(ptr.add(offset - 2 * chunk_bytes) as *const Chunk);
            let v = *(ptr.add(offset - chunk_bytes) as *const Chunk);

            // Break if there is a matching byte.
            let zu = contains_zero_byte(u ^ repeated_x);
            let zv = contains_zero_byte(v ^ repeated_x);
            if zu || zv {
                break;
            }
        }
        offset -= 2 * chunk_bytes;
    }

    // Find the byte before the point the body loop stopped.
    text[..offset].iter().rposition(|elt| *elt == x)
}
#[inline(never)]
    fn dump_var(
        f: usize,
        var0: usize, val0: impl PrintFDebug,
        var1: usize, val1: impl PrintFDebug,
        var2: usize, val2: impl PrintFDebug,
        var3: usize, val3: impl PrintFDebug,
    ) {
        unsafe{
            printf("fn%u:_%u = \0".as_ptr() as *const i8,f,var0);
            val0.printf_debug();
            printf("\n_%u = \0".as_ptr() as *const i8,var1);
            val1.printf_debug();
            printf("\n_%u = \0".as_ptr() as *const i8,var2);
            val2.printf_debug();
            printf("\n_%u = \0".as_ptr() as *const i8,var3);
            val3.printf_debug();
            printf("\n\0".as_ptr() as *const i8);
        }
    }
    trait PrintFDebug{
        unsafe fn printf_debug(&self);
    }
    impl PrintFDebug for u8{
        unsafe fn printf_debug(&self){
            printf("%u\0".as_ptr() as *const i8,*self as u8 as i32);
        }
    } 
    impl PrintFDebug for bool{
        unsafe fn printf_debug(&self){
            if *self{
                printf("true\0".as_ptr() as *const i8);
            }
            else{
                printf("false\0".as_ptr() as *const i8);
            }
        }
    } 
    impl PrintFDebug for (){
        unsafe fn printf_debug(&self){
            printf("()\0".as_ptr() as *const i8);
        }
    } 
    #[inline]
    pub(crate) const fn repeat_u8(x: u8) -> usize {
        usize::from_ne_bytes([x; core::mem::size_of::<usize>()])
    }
    #[inline]

const fn contains_zero_byte(x: usize) -> bool {
    x.wrapping_sub(LO_USIZE) & !x & HI_USIZE != 0
}
const LO_USIZE: usize = repeat_u8(0x01);
const HI_USIZE: usize = repeat_u8(0x80);