---
layout: post
title:  "Microbenchmarks Considered Useful"
description: ""
date: 2025-01-09
external_url: "https://frappe.io/blog/engineering/microbenchmarks-considered-useful"
---


> "Gosh darn it, you're not worried about microseconds... if you're using JavaScript and the next thing out of your mouth is 'I'm worried about microseconds', don't use JavaScript"
>
> -- [ThePrimeGen](https://www.youtube.com/shorts/4OoqBk3nhyY) (Professional YouTuber/Yapper)  [0]


Last month I started the practice of [Performance Engineering](https://github.com/frappe/caffeine) at Frappe with the singular goal of *improving performance of everything by 2x* (we'll revisit what this means later). I saw that Prime clip that while I was working on microbenchmarks that measured small Python operations in microseconds or even nanoseconds. So I felt obliged to counter this sentiment. It's not unique to Prime, it's a common sentiment on the web that microbenchmarks are not truthful and often meaningless. Before we dissect the usefulness of microbenchmarks, let's first understand some basics of microbenchmarking.


### What is microbenchmarking?

Microbenchmarks are small snippets of code, often just 2-10 lines that are run in a loop many times to get precise timing information. This is how you'd benchmark a very small function call:

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

Notice that each function call is not individually timed because that can lead to disturbance in the results if that function is very fast i.e. on the order of a few microseconds. Writing code like this manually for benchmarks is prone to errors and lack of repeatability. Picking the proper number for outer and inner loops for a given time budget is also something we can outsource to another program. So developers usually use utilities like Python's `timeit`, which takes care of estimating how many inner and outer loops are required for getting reliable and repeatable results.

### What is wrong with microbenchmarking?

Any program is the sum of its parts, then what is wrong with this way of timing small parts of the code?  That entirely depends on how that function is used inside your application. If that function is indeed called thousands of times in a tight loop then what you're measuring will closely resemble real-world performance. In every other case, these benchmarks tell you only a part of the whole picture or even mislead you in the wrong direction:

1. It's hard to extract small but representative pieces of code for benchmarking. You need to think of all the internals of the function calls and ensure that your benchmarking code is not unfairly benefiting when running in a loop. E.g. some computations might be memoized during a request, so you need to remove those caches in each loop to accurately benchmark it.
2. Running code in the loop means it's very warm in CPU caches and hardware optimizations like branch predictor and memory pre-fetcher can optimize execution of such loops. It's not uncommon to see small gains in microbenchmarks completely go away with realistic memory access patterns.
3. Poorly written benchmarks are also unlikely to exercise all code paths that would get exercised with different inputs in a realistic scenario.
4. If your language has Just-In-Time (JIT) compilation like JavaScript and recent versions of Python then the performance depends on how often a particular piece of code has run. You could be effectively benchmarking JIT-compiled native code that is interpreted in real usage.
5. It's just hard to get [repeatable results](https://ankush.dev/p/reliable-benchmarking). Trying to get a variance of <1% requires bare-metal hardware with sources of noise from hardware and other software removed. All of these techniques take you far from reality where your workload is most likely executing inside a VM.

### When to microbenchmark?

Despite all the criticism, microbenchmarks are everywhere. Surely, it's not *just* for marketing purposes?

The goal of *improving performance of everything by 2x* is quite vague and it's not even worth trying to explain what it means just yet. However you interpret it, one thing is for sure: we want big performance improvements. The first problem I had to deal with was "where to even begin?"

I know our codebase fairly well. I've worked on all 3 major projects at Frappe - Frappe Framework, ERPNext, Press, and many other smaller projects. So I have a pretty good idea of what is executing on the CPU *most of the time*. It won't be a surprise to read that a web framework made for business apps spends most of its time in:

- Database abstractions and SQL Queries
- Retrieving [frequently used data from the cache](https://ankush.dev/p/flamegraph-missing-forest-for-trees)
- ORM abstractions for CRUD operations
- Small utils that are used everywhere - configurations, math, datetime, etc

So I assumed it's best to isolate each of these in microbenchmarks and study what can be improved. I've profiled our codebase on many different workloads hundreds of times, so I am not going in blind here. I am just doing this with a different perspective.

I have had [1.1x goal](https://frappe.io/blog/engineering/reducing-memory-footprint-of-frappe-framework) before which I was able to achieve in a few weeks but 2x isn't as easy, as all low-hanging wins are already claimed. So my plan was to do 100x small 1.01x changes that add up to a big number and microbenchmarks are a *perfect* tool for this problem.

### Where do microbenchmarks shine?

This is all *my opinion* but I believe microbenchmarks shine when used in conjunction with continuous benchmarking with the benefit of preventing performance regressions. It's hard to isolate what change caused a 1% difference in overall performance but it's easy to spot a microbenchmark that showed a slowdown by 1.5x. Code reviews help a little here but they can not replace cold hard data. Take this diff for example:

```diff
- datetime.fromisoformat(string_date)
+ datetime.datetime.strptime(datetime_str, "%Y-%m-%d %H:%M:%S.%f")
```

Both lines are practically the same but `fromisoformat` is 10x faster because it's written in C. The most popular suggestions for date parsing online are still suggesting `strptime` or something even worse that tries to parse 10 different formats. One might dismiss the difference between 10 microseconds vs 1 microsecond as mIcRoOpTimizAtiOn, but when you have to frequently de-serialize 1000s of datetime objects from the database it quickly adds to be a significant number.

How about one more example?

```diff
 class attrdict(dict):

-    __getattr__ = dict.get

+    def __getattr__(self, k):
+        return super().get(k)
```

`attrdict` is a dictionary that allows accessing keys as an attribute i.e. you can do `dict.attr` instead of `dict[attr]`. The original implementation does it by referencing the same underlying method but the more explicit version does a function call using `super()` to indicate that attribute access is to be treated as key access. The explicit approach is 3x slower because of the additional function call. The difference is in *nanoseconds*, but this dictionary class is used EVERYWHERE in our codebase: documents, configuration, SQL query results... it's a long list.

So while the microbenchmark numbers don't mean anything on their own, changes to that number over time help maintain software performance.

### Conclusion

I spent last month writing microbenchmarks to uncover and fix ~50 such small performance problems without much care for the big picture (yet) but the big picture changed too. I checked ERPNext test run times which comprises ~2000 tests to ensure ERPNext correctness and that test suite was *somehow* 1.3x faster without any explicit work on it. That number later became 1.66x after some explicit optimizations for the test suite. Those tests used to take ~55 minutes to run and now take ~33 minutes, so I am not fooling myself here.

Like everything else in computer science, the answer to the usefulness of microbenchmarks is also *"it depends"*. I can see them being useful in conjunction with other performance measurement techniques like end-to-end benchmarks, load testing, and trace replay. Use whatever works for you, just don't try to fool someone into believing that microbenchmark numbers on their own mean something or worse...

> "The first principle is that you must not fool yourself, and you are the easiest person to fool."
>
> -- Richard Feynman

---

[0] I know that statement is out of context, but he also decided to upload that short clip out of context.
