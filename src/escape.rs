//! Simple json escaping

use std::fmt::Write;

pub trait CollectEscaped {
    /// Write escaped version of `self` to `out`
    fn collect_pango_into<T: Write>(self, out: &mut T);

    /// Write escaped version of `self` to a new buffer
    #[inline]
    fn collect_pango<T: Write + Default>(self) -> T
    where
        Self: Sized,
    {
        let mut out = T::default();
        self.collect_pango_into(&mut out);
        out
    }
}

impl<I: Iterator<Item = char>> CollectEscaped for I {
    fn collect_pango_into<T: Write>(self, out: &mut T) {
        for c in self {
            let _ = match c {
                '&' => out.write_str("&amp;"),
                '<' => out.write_str("&lt;"),
                '>' => out.write_str("&gt;"),
                '\'' => out.write_str("&#39;"),
                x => out.write_char(x),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pango() {
        let orig = "&my 'text' <>";
        let escaped: String = orig.chars().collect_pango();
        assert_eq!(escaped, "&amp;my &#39;text&#39; &lt;&gt;");
    }
}
