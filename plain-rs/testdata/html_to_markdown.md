# HTML to Markdown behavior contract

`html_to_markdown.json` contains 212 synthetic input/output cases. The same fixture is committed at `plain-app/shared/apitest/html_to_markdown.json` so each repository can run independently. The App contract test compares the two files when both repositories are present.

The baseline is the existing App `MDConverter` with its default options. Both implementations compare complete strings, including spaces and newlines. Coverage includes headings, paragraph boundaries, inline flanking spaces, nested formatting and lists, list starts, task inputs, links and images, code blocks and backticks, blockquotes, tables/sections/alignment/colspan/captions, escaping, every supported named entity, Unicode, comments, CDATA, attributes, and malformed markup.

Historical output is preserved, including UTF-16 heading underline lengths, single-tilde strikethrough, indented code, highlighted blocks containing their original `<pre>` markup, and incomplete CDATA consuming the internal root closing tag. This is a converter contract; it does not sanitize HTML or fetch articles.

Numeric entities in Rust use valid Unicode scalar values: supplementary characters are preserved, while invalid scalar references remain literal. This corrects the Kotlin parser's `toChar()` truncation and is separately locked by `numeric_entities_use_unicode_scalars`. Literal supplementary characters already match Kotlin and are covered by the shared fixture.

Feed integration tests additionally lock XML entity decoding and whitespace, description/content selection, relative links/images, missing assets, article extraction, and code indentation. RSS/Atom entities must be decoded once before HTML entity decoding.

Run Rust tests with `cargo test -p plain-rs --features html`. Run the App contract with `./gradlew --no-daemon :shared:testAndroidHostTest --tests 'com.ismartcoding.plain.lib.html2md.*'`.

Do not regenerate expected output from the Rust implementation. An intended behavior change requires reviewing the changed expected strings and updating both fixtures together.
