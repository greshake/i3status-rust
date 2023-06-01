//! Simple json escaping

use std::fmt::Write;

use unicode_segmentation::UnicodeSegmentation;

pub trait CollectEscaped {
    /// Write escaped version of `self` to `out`
    fn collect_pango_escaped_into<T: Write>(self, out: &mut T);

    /// Write escaped version of `self` to a new buffer
    #[inline]
    fn collect_pango_escaped<T: Write + Default>(self) -> T
    where
        Self: Sized,
    {
        let mut out = T::default();
        self.collect_pango_escaped_into(&mut out);
        out
    }
}

impl<I, R> CollectEscaped for I
where
    I: Iterator<Item = R>,
    R: AsRef<str>,
{
    fn collect_pango_escaped_into<T: Write>(self, out: &mut T) {
        for c in self {
            let _ = match c.as_ref() {
                "&" => out.write_str("&amp;"),
                "<" => out.write_str("&lt;"),
                ">" => out.write_str("&gt;"),
                "'" => out.write_str("&#39;"),
                x => out.write_str(x),
            };
        }
    }
}

pub trait Escaped {
    /// Write escaped version of `self` to `out`
    fn pango_escaped_into<T: Write>(self, out: &mut T);

    /// Write escaped version of `self` to a new buffer
    #[inline]
    fn pango_escaped<T: Write + Default>(self) -> T
    where
        Self: Sized,
    {
        let mut out = T::default();
        self.pango_escaped_into(&mut out);
        out
    }
}

impl<R: AsRef<str>> Escaped for R {
    fn pango_escaped_into<T: Write>(self, out: &mut T) {
        self.as_ref()
            .split_word_bounds()
            .collect_pango_escaped_into(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_pango() {
        let orig = "&my 'text' <a̐>";
        let escaped: String = orig.graphemes(true).collect_pango_escaped();
        assert_eq!(escaped, "&amp;my &#39;text&#39; &lt;a̐&gt;");
    }
    #[test]
    fn pango() {
        let orig = "&my 'text' <a̐>";
        let escaped: String = orig.pango_escaped();
        assert_eq!(escaped, "&amp;my &#39;text&#39; &lt;a̐&gt;");
    }
}
