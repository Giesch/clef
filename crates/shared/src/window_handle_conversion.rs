use std::ffi::c_void;

pub(crate) fn window_handle_token<E>(result: Result<*mut c_void, E>) -> Option<usize> {
    result
        .ok()
        .map(|handle| handle as usize)
        .filter(|handle| *handle != 0)
}

#[cfg(test)]
mod tests {
    use super::window_handle_token;

    #[test]
    fn windows_handle_conversion_success() {
        let pointer = std::ptr::without_provenance_mut(42);
        assert_eq!(window_handle_token(Ok::<_, ()>(pointer)), Some(42));
    }

    #[test]
    fn windows_handle_conversion_null() {
        assert_eq!(window_handle_token(Ok::<_, ()>(std::ptr::null_mut())), None);
    }

    #[test]
    fn windows_handle_conversion_error() {
        assert_eq!(
            window_handle_token(Err::<*mut std::ffi::c_void, _>(())),
            None
        );
    }
}
