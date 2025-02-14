# side

<img src="./images/sidechain_small.svg" width="25%">

side is a new text-processing utility that fills a gap between tools like sed, awk, grep, and xargs.

What distinguishes side from other tools is that its behavior is not controlled by
command-line flags or a bespoke scripting language. Instead, the user provides one or
more auxiliary line-processing commands whose outputs control the text stream.

side shines in these situations:
- your lines contain a mixture of encodings, e.g. base64 in TSV
- your filter mangles lines, but you need to preserve them
- you need to run a subprocess from an awk script, but can't afford to spawn a new process for every input line

side can also replace xargs for some use cases, and provides a key advantage: it does
not spawn a separate subprocess for every input line.

### "Side?"
The name and concept are borrowed from an audio mixing technique called
[side-chaining](https://en.wikipedia.org/wiki/Dynamic_range_compression#Side-chaining)
in which an effect applied to one audio signal is controlled by an auxiliary signal.

For example, you might run a bass guitar channel through a compressor controlled by a
kick drum channel. In this setup, the bass ducks out of the way on every kick drum
hit, making for a cleaner, punchier mix.

The auxiliary signal doesn't need to be from a different instrument. It could be a
filtered copy of the main signal. For example, a simple technique for [de-essing
vocals](https://en.wikipedia.org/wiki/De-essing#Side-chain_compression_or_broadband_de-essing)
is to use a compressor triggered by a high-pass-filtered copy of the vocal channel.

### Side-chaining in Unix Pipelines
In a data pipeline, we can use this technique to control our primary data path using
an auxiliary command or pipeline.

<img src="./images/sidechain_filter.svg" width="75%">

## Filter Mode
Sometimes, you can't build the filter you need without removing critical parts of
your input.

With `side filter`, you get to keep your original data, even if you use a
line-mangling filter.

### Example
Imagine we have lines of JSON-in-TSV:
```txt
# input.tsv
alice	{"foo":0,"bar":1}
billy	{"foo":1,"bar":1}
charlie	{"bar":0,"foo":1}
```
We want to filter this data to produce a list of users who have `.foo != .bar`. We
could use:
```bash
$ cut -f2 input.tsv | jq -c 'select(.foo != .bar)'
{"foo":0,"bar":1}
{"bar":0,"foo":1}
```
...but then we'd lose the usernames. What if we could use this filter _and_ keep our
original data?

#### Solution with side
```bash
$ side filter -x 'cut -f2 | jq ".foo != .bar"' -p true < input.tsv
alice	{"foo":0,"bar":1}
charlie	{"bar":0,"foo":1}
```
Arguments:
* `-x 'cut -f2 | jq ".foo != bar"'`: the aux command; this prints `true` when `.foo != .bar`.
* `-p true`: retain each line only if its aux output matches the pattern `true`.

<img src="./images/sidechain_filter_annotated.svg">

Here, we're telling side to start the aux command, pipe each line to it and look for
the pattern `true`. Matching lines are emitted **in their original, unmangled form.**

Note: the aux command is **spawned only once**. It's a long-running subprocess that
handles all input lines. Contrast this with a solution in `awk` or `bash`, which
would require invoking `jq` separately for every input line.

## Map Mode
In map mode, the aux command generates values which can be merged back into the main
pipeline.

<img src="./images/sidechain_map.svg" width="75%">

### Example
Suppose you have a file containing lines of JSON with a `"url"` field, and you want
to add a new `"host"` field containing the host component of the URL.

```json
{"name":"alice","url":"https://foo.com"}
{"name":"billy","url":"http://1.2.3.4:8000/api"}
```

It's not hard to extract the host. But how would you surgically do it for a URL
embedded in JSON?

#### Solution with side
For readability, let's use an imaginary tool called `host-from-url` to extract the
hosts. In reality, you could use the Ruby one-liner
`ruby -r uri -ne 'u = URI($_.chomp); puts(u.host || "")'` (this reads from stdin and
parses all of the URLs with a single invocation).

```bash
<input.json side map -I% -x 'jq .url | host-from-url' jq '.host = "%"'

#                           ^----- aux command -----^ ^-- main cmd --^
```
Like with xargs, we can use `-I <replstr>` to define a placeholder for the values
generated by the aux command.

<img src="./images/sidechain_map_example.svg" width="75%">

Here, the aux command `jq .url | host-from-url` extracts the hosts, which are
then inserted into the output of the main command, `jq '.host = "%"'`.

Remember, the aux and main commands are both long-running processes. They are not
invoked for every line, as xargs would do.

## Using `$[]`
For cleaner, more-intuitive interpolation, you can use `$[]` to embed your aux
command in your main one:

```bash
side map jq '.host = "$[jq .url | host-from-url]"' < input.json

#                       ^------ aux cmd ------^
#        ^------------- main command ------------^
```

<img src="./images/sidechain_map_example_interp.svg" width="75%">

This has the same behavior as the `-I%` version; it's just another way to spell it.

## Multiple Aux Commands
Map mode supports the use of _multiple aux commands_.

Continuing with the URL-parsing example, imagine you want to extract the port from
the URL as well. Again, we'll use a placeholder (`port-from-url`) instead of a real
command that extracts ports from URLs.

```bash
side map jq '
    .host = "$[jq .url | host-from-url]"
  | .port =  $[jq .url | port-from-url]
' < input.json
```

<img src="./images/sidechain_map_multiple.svg">

This is great, but it duplicates some work: we're running two copies of `jq .url`.

To prevent this, you can insert a preliminary aux command that feeds into the
downstream ones:

```bash
side map \
  -x 'jq .url' \
  jq '.host = "$[host-from-url]" | .port = $[port-from-url]' \
  < input.json
```

<img src="./images/sidechain_map_multiple_prelim.svg">
