---
layout: post
title:  "Missing the Forest for the Trees With Flame Graphs"
description: "Overhead demons can be hiding in plain sight in pretty flame graphs."
date: 2024-12-29
discussions:
  Lobsters: "https://lobste.rs/s/y62lgp/flame_graphs_can_hide_small_overheads"
  LinkedIn: "https://www.linkedin.com/posts/ankushmenat_missing-the-forest-for-the-trees-with-flame-activity-7279499667296407553-9DfF"
  HackerNews: "https://news.ycombinator.com/item?id=42555597"
---

<br>

<style>

.flamegraph-container {
  width: 100%;
  @media (min-width: 1220px) {
    width: 160%;
    position: relative;
    left: -30%;
  }
}

/* ref: https://www.w3schools.in/html/marquee-tag */
marquee {
    animation: blinker 1.5s linear infinite;
    font-family: "Courier", "Ubuntu Mono", "Consolas", "Monaco", monospace;
}

@keyframes blinker {
    50% {
        opacity: 0.5;
    }
}
</style>


<!--
I always wanted to do this. It looks so cool.
I'm sorry this gives gives you PTSD about IE6 era web. Thankfully we have great things like Safari now!
-->
<marquee>
This post is best viewed on a desktop with 1920x1080 or higher resolution.
</marquee>

[Flame graphs](https://brendangregg.com/flamegraphs.html) are an amazing tool to visualize the performance of software and I'll forever be grateful to Brendan Gregg for creating them. There is however one catch that you should be aware of though. They tend to hide small overheads that have a bigger overall impact very well.


Let's look at a real example. The following flame graph shows a web worker under a very common CRUD operation - read one document from the database and send it back as a JSON response. Currently highlighted stack is the actual operation of loading a document from the database; the rest are all pure overheads - authentication, rate limiting, serialization, etc.


<object data="/assets/images/getdoc_flamegraph.svg?s=read_doc" type="image/svg+xml" class="flamegraph-container">
  <img src="/assets/images/getdoc_flamegraph.svg" />
</object>


Theoretically, I can speed this up by 2x if I eliminate all of the overheads. Naturally, one way to approach this would be to inspect each component of this overhead, profile them separately, and reduce or eliminate them.... right? That will work but it will also be pretty inefficient.

Think of any utility functions that you might have in your application like math utilities, database, or cache abstractions... they are used everywhere but they are rarely used higher up in the call stacks.


To see these overheads, we need to merge stacks bottom-up instead of typical top-down merging. I didn't find this option readily available in tools like `py-spy` but all of them usually allow dumping raw data which can be used to generate these graphs using the original `flamegraph.pl` script. So we can use the `--reverse` flag to get a different view.

```
flamegraph.pl --reverse raw_input.txt > reversed.svg
```


<object data="/assets/images/getdoc_flamegraph_reversed.svg?s=redis" type="image/svg+xml" class="flamegraph-container">
  <img src="/assets/images/getdoc_flamegraph_reversed.svg?s=redis" />
</object>


Well, shiiit. Redis calls is where I should be spending my time. I need a _cache for our cache_ and something like [client-side caching](https://redis.io/docs/latest/develop/reference/client-side-caching/) can possibly eliminate 80% of this work.


P.S: This doesn't always seem to work because flame graphs are typically used with sampling profilers. Sampling profilers can reliably identify work done higher up in call stacks but that reliability quickly drops down the lower you go in the call stack. Take some time to ponder about why it works this way. Few ways to address this problem:
- Increase the sampling rate.
- Use a tracing profiler.
- Just manually inspect the tips of flames in top-down flame graphs and use search to highlight possible suspects.

P.P.S.: These flame graphs are interactive on _modern_ browsers. Try to search for "redis" in the original top-down flame graph.
