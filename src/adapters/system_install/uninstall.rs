//! Managed CLI removal facade.

mod ownership;
mod path_registration;
#[cfg(windows)]
mod windows_cleanup;

pub(crate) use ownership::{
    binary_removal_plan, remove_installed_binary, validate_clean_uninstall_targets,
};
pub(super) use ownership::{install_is_owned, record_install_ownership};
#[cfg(test)]
pub(super) use ownership::{install_owner_file, BinaryRemovalResult};

#[cfg(test)]
pub(super) use path_registration::render_profile_without_managed_block;
pub(crate) use path_registration::{remove_user_path, user_path_removal_plan};
#[cfg(windows)]
pub(super) use path_registration::{windows_path_owner_file, windows_path_removal};
