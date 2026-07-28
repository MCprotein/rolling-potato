mod find;
mod open;

#[cfg(test)]
pub(crate) use find::find_in_page;
pub(crate) use find::observe_find_in_page;
pub(crate) use open::observe_open_page;

#[cfg(test)]
mod tests;
