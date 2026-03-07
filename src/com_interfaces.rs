#![allow(non_snake_case, non_upper_case_globals, unused_imports)]
use windows::core::{GUID, HRESULT, PCWSTR, interface};

#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
pub unsafe trait IPolicyConfig: windows::core::IUnknown {
    unsafe fn GetMixFormat(
        &self,
        pszdevicename: PCWSTR,
        ppformat: *mut std::ffi::c_void,
    ) -> HRESULT;
    unsafe fn GetDeviceFormat(
        &self,
        pszdevicename: PCWSTR,
        bdefault: i32,
        ppformat: *mut std::ffi::c_void,
    ) -> HRESULT;
    unsafe fn ResetDeviceFormat(&self, pszdevicename: PCWSTR) -> HRESULT;
    unsafe fn SetDeviceFormat(
        &self,
        pszdevicename: PCWSTR,
        pendpointformat: *const std::ffi::c_void,
        pmixformat: *const std::ffi::c_void,
    ) -> HRESULT;
    unsafe fn GetProcessingPeriod(
        &self,
        pszdevicename: PCWSTR,
        bdefault: i32,
        pmftdefaultperiod: *mut i64,
        pmftminimumperiod: *mut i64,
    ) -> HRESULT;
    unsafe fn SetProcessingPeriod(&self, pszdevicename: PCWSTR, pmftperiod: *const i64) -> HRESULT;
    unsafe fn GetShareMode(&self, pszdevicename: PCWSTR, pmode: *mut i32) -> HRESULT;
    unsafe fn SetShareMode(&self, pszdevicename: PCWSTR, mode: i32) -> HRESULT;
    unsafe fn GetPropertyValue(
        &self,
        pszdevicename: PCWSTR,
        pkey: *const std::ffi::c_void,
        pvalue: *mut std::ffi::c_void,
    ) -> HRESULT;
    unsafe fn SetPropertyValue(
        &self,
        pszdevicename: PCWSTR,
        pkey: *const std::ffi::c_void,
        pvalue: *const std::ffi::c_void,
    ) -> HRESULT;
    unsafe fn SetDefaultEndpoint(&self, wszdeviceid: PCWSTR, role: i32) -> HRESULT;
    unsafe fn SetEndpointVisibility(&self, wszdeviceid: PCWSTR, bvisible: i32) -> HRESULT;
}

#[allow(non_upper_case_globals)]
pub const CLSID_PolicyConfig: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

pub fn set_default_device(device_id: &str) -> bool {
    unsafe {
        use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};

        let wide_id: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        let pcwstr = PCWSTR(wide_id.as_ptr());

        if let Ok(policy_config) =
            CoCreateInstance::<_, IPolicyConfig>(&CLSID_PolicyConfig, None, CLSCTX_ALL)
        {
            // Role 0 = eConsole, 1 = eMultimedia, 2 = eCommunications
            let _ = policy_config.SetDefaultEndpoint(pcwstr, 0);
            let _ = policy_config.SetDefaultEndpoint(pcwstr, 1);
            let _ = policy_config.SetDefaultEndpoint(pcwstr, 2);
            return true;
        }
    }
    false
}
