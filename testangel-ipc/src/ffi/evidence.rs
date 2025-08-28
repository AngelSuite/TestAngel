use std::ffi::c_char;

#[repr(C)]
pub struct ta_evidence {
    pub szLabel: *const c_char,
    pub kind: ta_evidence_kind,
    pub value: *const c_char,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub enum ta_evidence_kind {
    TA_EVIDENCE_TEXTUAL = 0,
    TA_EVIDENCE_PNGBASE64 = 1,
    TA_EVIDENCE_HTTPREQRES = 2,
}
