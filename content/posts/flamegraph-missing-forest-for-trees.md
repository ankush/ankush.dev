---
layout: post
title:  "Missing the Forest for the Trees With Flamegraphs"
description: "Overhead demons can be hiding in plain sight in pretty flamegraphs."
date: 2024-12-29
discussions:
---

[Flame graphs](https://brendangregg.com/flamegraphs.html) are an amazing tool to visualize the performance of software and I'll forever be grateful to Brendan Gregg for creating them. There is however one catch that you should be aware of though. They tend to hide small overheads that have a bigger overall impact very well.


Let's look at a real example. The following flame graph shows a web worker under a very common CRUD operation - read one document from the database and send it back as a JSON response. Currently highlighted stack is the actual operation of loading a document from the database; the rest are all pure overheads - authentication, rate limiting, serialization, etc.

<object data="/assets/images/getdoc_flamegraph.svg?s=read_doc" type="image/svg+xml" style="width: 100%">
  <img src="/assets/images/getdoc_flamegraph.svg" />
</object>


Theoretically, I can speed this up by 2x if I eliminate all of the overheads. Naturally, one way to approach this would be to inspect each component of this overhead, profile them separately, and reduce or eliminate them.... right? That will work but it will also be pretty inefficient.

Think of any utility functions that you might have in your application like math utilities, database, or cache abstractions... they are used everywhere but they are rarely used higher up in the call stacks.


To see these overheads, we need to merge stacks bottom-up instead of typical top-down merging. I didn't find this option readily available in tools like `py-spy` but all of them usually allow dumping raw data which can be used to generate these graphs using the original `flamegraph.pl` script. So we can use the `--reverse` flag to get a different view.

```
flamegraph.pl --reverse raw_input.txt > reversed.svg
```


<object data="/assets/images/getdoc_flamegraph_reversed.svg?s=redis" type="image/svg+xml" style="width: 100%">
  <img src="/assets/images/getdoc_flamegraph_reversed.svg?s=redis" />
</object>

Well, shiiit. Redis calls is where I should be spending my time. I need a _cache for our cache_ and something like [client-side caching](https://redis.io/docs/latest/develop/reference/client-side-caching/) can help here.

P.S.: These flamegraphs are interactive on _modern_ browsers. Try to search for "redis" in the original top-down flamegraph.

P.P.S: This doesn't always seem to work because flamegraphs are typically used with sampling profilers. Sampling profilers can reliably identify work done higher up in call stacks but that reliability quickly drops down the lower you go in the call stack. Take some time to ponder about why it works this way. Few ways to address this problem:
- Increase the sampling rate.
- Use a tracing profiler.
- Just manually inspect the tips of flames in top-down flame graphs and use search to highlight possible suspects.

