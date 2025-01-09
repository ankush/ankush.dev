---
layout: post
title:  "Microbenchmarks NOT Considered Harmful"
description: ""
date: 2025-01-09
external_url: "https://frappe.io/blog/engineering/TBD"
published: false
---


> "Gosh darn it, you're not worried about microseconds... if you're using JavaScript and the next thing out of your mouth is 'I'm worried about microseconds', don't use JavaScript"
>
> -- [ThePrimeGen](https://www.youtube.com/shorts/4OoqBk3nhyY) [0] (Professional YouTuber/Yapper)



This isn't unique to Prime, it's a common sentiment on the web that microbenchmarks are not truthful and often meaningless. Before we dissect the usefulness of microbenchmarks, lets first understand some basics of microbenchmarking.


### What is microbenchmarking?


Microbenchmarks are small snippet of code, often just 2-10 lines that are ran in a loop many-many times to get precise timing information. This is how you'd benchmark a very small function call:

```python
measurements = []
for _ in range(20):          # outer loop
    start = time.monotonic()
    for _ in range(100):     # inner loop
        func()               # function of interest
    end = time.monotonic()
    avg_time = (end - start) / 100
    measurements.append(avg_time)
```

Notice that each function call is not individually timed because that can lead to disturbance in the results if that function is very fast i.e. on the order of few microseconds. Writing code like this manually for benchmarks is prone to many errors and lack of repeatability. Picking proper number for outer and inner loops is also something we can outsource to another program. So developers usually use utilities like Python's `timeit`, which takes care of estimating how many inner and outer loop are required for getting reliable and repeatable results.

### What is wrong with microbenchmarking?

Any program is sum of its parts, then what is wrong with this way of timing small parts of the code?  That entirely depends on how that function is used inside your application. If that function is indeed called thousands of time in a tight loop then what you're measuring will closely resemble real-world performance. In every other case, these benchmarks tell you only a part of the whole picture or even mislead you into wrong direction:

1. It's hard to extract small but representative piece of code for benchmarking. You need to think of all the internals of the function calls and ensure that your benchmarking code is not unfairly benefiting when running in a loop. E.g. some computations might be memoized during a request, so you need to remove those caches in each loop to accurately benchmark it.
2. Running code in the loop means it's very warm in CPU caches and hardware optimizations like branch predictor and memory pre-fetcher can optimize execution of such loops. It's not uncommon to see small gains in microbenchmark completely go away with realistic memory access patterns.
3. Poorly written benchmarks are also unlikely to exercise all code paths that would get exercised with different inputs in realistic scenario.
4. If your language has Just-In-Time (JIT) compilation like JavaScript and recent versions of Python then the performance depends on how often a particular piece of code has ran. You could be effectively benchmarking JIT-compiled native code that is actually interpreted in real usage.
5. It's just hard to get [repeatable results](https://ankush.dev/p/reliable-benchmarking). Trying to get variance of <1% requires bare-metal hardware with sources of noise from hardware and other software removed. All of these measure take you far from reality where your workload is most likely executing inside a VM where system calls have different costs than bare-metal installations.

### When to microbenchmark?

Despite all the criticism, microbenchmarks are everywhere. Surely, it's not *just* for marketing purposes?

I started the practice of ["Performance Engineering"](https://github.com/frappe/caffeine) at Frappe last month with singular goal of *improving performance by 2x*. That is a vague statement and I am not even going to explain what it means yet. The first problem I had deal with was "where to even begin?"

I know our codebase fairly well. I've worked on all 3 major projects at Frappe - Frappe Framework, ERPNext, Press and many other smaller projects. So I have a pretty good idea of what is executing on CPU *most of the time*. It won't be a surprise to read that a Web Framework made for business apps spends most of its time in:

- Database abstractions and SQL Queries
- Retrieving frequently used data from cache
- ORM abstractions for CRUD operations
- Small utils that are used everywhere - configurations, math, datetime etc

So I assumed it's best to isolate each of these in microbenchmarks and study what can be improved. I've obviously profiled our codebase on many different workloads hundreds of times, so I am not going in blind here. I am just doing this with a different perspective. I have had [1.1x goal](https://frappe.io/blog/engineering/reducing-memory-footprint-of-frappe-framework) before which I was able to achieve in a few weeks but 2x isn't as easy, as all low-hanging wins are already claimed. So my plan was to do 100x small 1% changes that add up to a big number and microbenchmarks are a *perfect* tool for this problem.

### Where microbenchmarks shine?

This is all *my opinion* but I believe microbenchmarks clearly shine when used in conjunction with continuous benchmarking with benefit being prevention of performance regressions. It's hard to isolate what change caused 1% difference in overall performance but it's easy to spot a microbenchmark that showed a slowdown by 1.5x. Code reviews help a little here but they can not replace cold hard data. Take this diff for example:

```diff
- datetime.fromisoformat(string_date)
+ datetime.datetime.strptime(datetime_str, "%Y-%m-%d %H:%M:%S.%f")
```

Both lines are practically same but `fromisoformat` is 10x faster because it's written in C. Most popular suggestion for date parsing online are still suggesting `strptime` or something even worse that tries to parse 10 different formats. One might totally dismiss difference between 10 microseconds vs 1 microseconds as mIcRoOpTimizAtiOn, but when you have to frequently de-serialize 1000s of datetime objects from database it quickly adds to be a significant number.

How about one more example?

```diff
 class attrdict(dict):

-    __getattr__ = dict.get

+    def __getattr__(self, k):
+        return super().get(k)
```

`attrdict` is a dictionary that allows accessing keys as attribute i.e. you can do `dict.attr` instead of `dict[attr]`. Original implementation does it by referencing same underlying method but the more explicit version does a function call using `super()` to indicate that attribute access is to be treated as key access. The explicit approach is 3x slower because of additional function call. The difference is in *nanoseconds*, but this dictionary class is used EVERYWHERE in our codebase: documents, configuration, SQL query results... it's a long list.

So while the microbenchmark numbers don't mean anything on their own, changes to that number over time are clearly helpful in maintaining software performance.

### Conclusion

I spent last month writing microbenchmarks to uncover and fix ~50 such small performance problems without much care for the big picture (yet) but the big picture changed too. I checked ERPNext test run times which comprises of ~2000 tests to ensure ERPNext correctness and that test suite was *somehow* 1.3x faster without any explicit work on it. That number later became 1.66x after some explicit optimizations for the test suite. Those tests used to take ~55 minutes to run and now take ~33 minutes, so I am definitely not fooling myself here.

Like everything else in computer science, the answer to usefulness of microbenchmarks is also *"it depends"*. I can see them being useful in conjunction with other performance measurements techniques like end-to-end benchmarks, load testing and trace replay. Use whatever works for you, just don't try to fool someone into believing that microbenchmark numbers on their own mean something, or worse...

> "The first principle is that you must not fool yourself, and you are the easiest person to fool."
>
> -- Richard Feynman


---


[0] I know that statement is out of context, but he decided to upload that short clip out of context so ...
