# xcopr
<img src="./images/xcopr_small.svg" width="25%">

`xcopr` adds ergonomic **coprocessing** to the classic Unix toolkit.

Like `xargs`, `xcopr` plays a supporting role, allowing users to compose familiar
tools more easily.

Unlike `xargs`, `xcopr` is focused primarily on data streams: it empowers users to
split and rejoin them, increasing the reach of shell-based stream processing.

## What is a coprocess?
A coprocess runs in parallel with a main process and communicates bidirectionally
with it.

Coprocesses are often overlooked in shell pipelines, understandably so: it's not easy
to use them that way.
[Bash](https://www.gnu.org/software/bash/manual/html_node/Coprocesses.html),
[ksh](https://www.ibm.com/docs/en/aix/7.1?topic=shell-coprocess-facility), and
[gawk](https://www.gnu.org/software/gawk/manual/html_node/Two_002dway-I_002fO.html)
have coprocessing features, but they are too verbose to serve as pipeline building
blocks in practice.

Whereas `xargs` converts input streams into batches of arguments to be passed to a
series of subprocesses, `xcopr` spins up one or more long-lived coprocesses, pipes
data to them, and merges their outputs into the main data path in a user-specified
way.

## What is `xcopr` good for?
`xcopr` may help in these situations:
- Your data contains a mixture of encodings (e.g., base64 in TSV).
- Your pipeline involves piping queries to a database client like `sqlite3` or
  `redis-cli`.
- You want to use a line-mangling filter (like `cut` or `jq`) but need to preserve
  the original lines for subsequent steps in the pipeline.
- You're using xargs or awk to run subprocesses, but don’t want to fork a new process
  per line.
- You want compose tools in a seemingly-impossible way (e.g., splitting a pipeline
  into multiple branches).

# Modes
## `xcopr filter`
In filter mode, the user specifies a filter to be executed as a coprocess whose
output will determine whether each line passes through.

Lines are passed through in their original form, unaffected by any destructive
line-mangling performed by the filter.

<img src="./images/xcopr_filter.svg" width="75%">

### Example
Imagine we have lines of JSON-in-TSV:
```txt
# input.tsv
alice	{"foo":0,"bar":1}
billy	{"foo":1,"bar":1}
charlie	{"bar":0,"foo":1}
```
We want to filter this data to produce a list of users who have `.foo == .bar`. We
could use:
```bash
$ cut -f2 input.tsv | jq -c 'select(.foo == .bar)'
{"foo":1,"bar":1}
```
...but then we'd lose the usernames. With `xcopr`, we get to keep the original data
by isolating the line-mangling to a coprocess.

#### Solution with `xcopr filter`
(`xcopr f`, for short)
```bash
$ xcopr f -c 'cut -f2 | jq ".foo == .bar"' -e true < input.tsv
billy	{"foo":1,"bar":1}
```
Arguments:
* `-c COMMAND`: a coprocess; this one happens to print `true` when `.foo == .bar`.
* `-e PATTERN`: output lines whose coprocess output matches the pattern `true`.

<img src="./images/xcopr_filter_annotated.svg">

Here, we're telling `xcopr` to start the coprocess, pipe each line to it, and look
for the pattern `true` in its output. Matching lines are emitted **in their original,
unmangled form.**

Remember: the coprocess is **spawned only once**. It's a long-running program that
handles all input lines. Contrast this with a traditional shell loop, which would
invoke `jq` separately for every line.

## `xcopr map`
In map mode, the coprocess generates values which can be injected back into the main
process's output.

<img src="./images/xcopr_map.svg" width="75%">

### Example
Suppose you have a SQL database containing DNS-like data:
```
sqlite> select * from dns;
id  domain   ip
--  -------  -------
1   foo.com  1.1.1.1
2   bar.com  2.2.2.2
3   baz.com  3.3.3.3
```
Suppose you need to use this database to enrich a large set of domains, annotating
them with their associated IP:
```
# input
{"domain":"foo.com"}
{"domain":"bar.com"}
...

# output
{"domain":"foo.com","ip":"1.1.1.1"}
{"domain":"bar.com","ip":"2.2.2.2"}
...
```
A reasonable implementation might look like this:
```bash
while read -r line; do
  dom=$(echo $line | jq -r .domain)
  ip=$(sqlite3 -list test.db "SELECT ip FROM dns WHERE domain = '$dom'")
  echo $line | jq -c ".ip = \"$ip\""
done < domains.jsonl
```
However, this forks a new `sqlite3` process for every input line, even though
`sqlite3` is capable of bulk queries via stdin.

You could ditch `jq` and do all the formatting using SQLite's built-in JSON support,
but this gets hairy quickly.

This type of problem follows a pattern:
- The original solution uses multiple tools composed with a shell loop or `xargs`.
- You could improve performance, but it would require consolidating all of the logic
  into one tool.
- The result is brittle and verbose. The solution is more complex than the problem
  because the most natural tools cannot be composed in the required way.

#### Solution with `xcopr map`
With `xcopr map`, you get the performance of bulk queries and keep the elegance
afforded by composing small tools.

```bash
xcopr map \
  -c "jq -r .domain" \
  -p "SELECT ip FROM dns WHERE domain = '%1'" \
  -c "%2{sqlite3 -list test.db}" \
  jq -c '.ip = "%3"'
```
|Argument|Explanation|Stream|
|--------|-----------|------|
|`-c "jq -r .domain"`|A coprocess that produces a stream of domains|1|
|`-p "SELECT ip FROM dns WHERE domain = '%1'"`|Creates a stream of strings (`-p` is for `--print`). This is a quick, native substitute for a coprocess like `awk {print ...}`. The `%1` is a placeholder similar to `\1` in `sed` or `$1` in `awk`: it takes on a different value for each input line. In `%N`, the `N` refers to the Nth stream specified.|2|
|`-c "%2{sqlite3 -list test.db}"`|A coprocess that produces a stream of IPs returned by the SQLite database. The `%N{}` syntax means: "send stream `N` to the `{}`-enclosed coprocess.|3|
|`jq -c '.ip = "%3"'`|The main process. This receives the original stdin and injects the IPs from stream 3|-|
