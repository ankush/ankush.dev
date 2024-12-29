---
layout: post
title:  "Missing the Forest for the Trees with Flamegraphs"
description: "Overhead demons can be hiding in plain sight in pretty flamegraphs."
date: 2024-12-28
discussions:
---


Flamegraphs are an amazing tool to visualise performance of software and I'll forever be grateful to Brendan Gregg for creating them. There is however one catch that you should be aware of though. They tend to hide small overheads that have bigger overall impact very well.


Let's take a real example. Following flamegraph shows a web worker under a very common CRUD operation - Read one document from database using REST API. Currently highlighted stack is the actual operation of loading a document from database, rest is all pure overheads - authentication, rate limiting, configuration loading etc etc.

<object data="/assets/images/getdoc_flamegraph.svg?s=read_doc" type="image/svg+xml" style="width: 100%">
  <img src="/assets/images/getdoc_flamegraph.svg" />
</object>

So if I want to speed this up by 2x I'd naturally try to eliminate each of these overheads one by one right? That will work but it will also be pretty inefficient.

Think of any utility functions that you might have in you application like math utilities, database or cache abstractions... these are used everywhere but they are rarely used higher up in the call stacks.


To see these overheads, we need to merge stacks bottom-up instead of typical top-down. I didn't find this option readily available in tools like py-spy but they allow dumping raw data which can be used to generate these graphs using `flamegraph.pl` script and `--reverse` flag.


<object data="/assets/images/getdoc_flamegraph_reversed.svg?s=redis" type="image/svg+xml" style="width: 100%">
  <img src="/assets/images/getdoc_flamegraph_reversed.svg?s=redis" />
</object>

Well shit. Redis calls is where I should be spending my time on.  [client-side caching](https://redis.io/docs/latest/develop/reference/client-side-caching/) I can recover a lot of this wastage.


P.S.: These flamegraphs are interactive on _modern browsers_, try to search for "redis" in the original top-down flamegraph.


