//! Application UI boundary for type checking the real choice coordinator. No windows are created.
pub trait Runtime: Send + Sync + 'static {}
pub struct TestRuntime;
impl Runtime for TestRuntime {}
pub struct AppHandle<R: Runtime>(std::marker::PhantomData<R>);
impl<R: Runtime> Clone for AppHandle<R> {
    fn clone(&self) -> Self {
        Self(std::marker::PhantomData)
    }
}
pub struct WebviewWindow<R: Runtime>(std::marker::PhantomData<R>);
impl<R: Runtime> WebviewWindow<R> {
    pub fn label(&self) -> &str {
        "sidebar"
    }
    #[cfg(windows)]
    pub fn hwnd(&self) -> Result<windows::Win32::Foundation::HWND, String> {
        Err("boundary".into())
    }
}
pub trait Manager<R: Runtime> {
    fn get_webview_window(&self, _: &str) -> Option<WebviewWindow<R>>;
}
impl<R: Runtime> Manager<R> for AppHandle<R> {
    fn get_webview_window(&self, _: &str) -> Option<WebviewWindow<R>> {
        None
    }
}
pub mod async_runtime {
    pub fn spawn<F: std::future::Future<Output = ()> + Send + 'static>(_f: F) {}
}

pub struct AppPath;
impl AppPath { pub fn app_data_dir(&self)->Result<std::path::PathBuf,String>{Err("boundary".into())} }
impl<R: Runtime> AppHandle<R> { pub fn path(&self)->AppPath{AppPath} }
