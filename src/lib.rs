extern crate uuid;

pub mod actors;
pub mod mailbox;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
