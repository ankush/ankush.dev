---
layout: post
title:  'Fake Debugging II: The Race Condition Strikes Back'
description: "Fake Debugging Considered Harmful."
date: 2024-12-25
discussions:
    HackerNews: "https://news.ycombinator.com/item?id=42509088"
    LinkedIn: "https://www.linkedin.com/posts/ankushmenat_fake-debugging-ii-the-race-condition-strikes-activity-7277678826703179776-URvk"
    Lobsters: "https://lobste.rs/s/rpl1vc/how_i_debugged_2_year_old_fake_debugged"
---


> "અધૂરું જ્ઞાન હોવા કરતા કશું ન જાણવું વધુ સારું છે."
>
> "It's better to know nothing than to have half-knowledge." (translation)
>
> -- Frequently forwarded WhatsApp "wiSdoM"

This is a story of how I "fake debugged" one of the worst multi-threading bugs we saw at work around 2 years ago and then how I _really_ debugged it yesterday.


### First signs of trouble

Multiple users at work reported getting `column "name" is ambiguous` errors randomly while doing anything. This is an error reported by MySQL when you use a column name without specifying which table it belongs to and if it's present in more than one table in the query.

Some context:
- I work on [Frappe Framework](https://frappe.io/framework), which is a "full stack" web framework like Django or Ruby On Rails but specifically designed for writing business apps like [ERPNext](https://erpnext.com/). You don't need to know much about these to read the rest of the post.
- Business apps tend to have a lot of queries, we have **thousands** of unique queries in our codebase.
- This bug is in the thread-unsafe implementation of our SQL query builder abstraction.

### First encounter and a temporary fix

First, I started by looking at error logs to see if there was any common pattern. All of these errors were caused by weird queries that no one would ever write themselves.

Consider this query for example:

```sql
SELECT "scopes"
FROM "OAuth Token"
LEFT JOIN "Pick List"  👈 What ???
    ON "Pick List"."parent" = "OAuth Token"."name"
WHERE
    "name" = "..."
LIMIT 1
```

"Pick List" is a document for warehouse operators to work on order fulfillment. Why would anyone ever join the OAuth table with the Pick List table? This query makes no sense.

Whenever I am debugging crazy bugs like these I leave breadcrumbs on the ticket so I or someone else can later revisit it. This was my first internal comment on the ticket:

> "Something very bad is going on. Random queries have joins with random tables which don't make any sense. Probably a QB (Query Builder) bug."


Then I remembered we had experimentally enabled multi-threaded web workers for some users, the default is multiprocessing-based web workers. This happened before we started using Sentry for centralized error reporting, so I had to manually check if all of them were using multi-threaded web workers and as expected, they were.

So I quickly rolled back the configuration change for them and monitored errors for a while. Error logs stopped completely. So, that was a very good hint that it was indeed a multi-threading bug.

### First (incorrect) diagnosis

After rolling back the change, now I had enough time to investigate it in depth. We use Python, so thread-safety issues mostly boil down to:

1. Mutable Global Variables
2. Mutable Class Attributes (which are shared with all instances)

I started reading query builder code and sure enough, I found _**exactly**_ what I was looking for:

```python
class Engine:  # Query building "engine"
    tables = {}   👈 Shared mutable state.
    ...
```

The query-building engine had a shared mutable class attribute called `tables` and this attribute is responsible for storing all the joins to be made while constructing the final query. So I just assumed it must be this since ALL the signs point to it?

1. A global shared mutable object
2. The object is responsible for joining queries and we have a problem with random tables getting joined.

I changed the code to remove the shared attribute and use an instance attribute instead.

```python
class Engine:
    def __init__(self):
        self.tables = {}  👈 This is local to the instance.
    ...
```

I was never quite able to "reproduce" this issue in a clean environment and no way I was gonna ask users to face this again, so I just assumed we solved it and moved on.

Since we _ALSO_ rolled back the multi-threading configuration change this problem never resurfaced.

### Doubts about the first diagnosis

While discussing this fix with other engineers (since a lot of us got random tickets related to this bug in different products), A colleague of mine pointed out that even though there is this shared mutable attribute, it's always overridden by instance attribute in code:

```python
def get_query(self, ...) -> Query:
    # Clean up state before each query
    self.tables = {}
```

My memory is a bit hazy but if I recall correctly we didn't investigate it further thinking this was some side effect of having an instance attribute shadowing a class attribute with the same name. Since I was not able to reproduce the problem in the first place, how would I even go about validating this? Spoiler: I was of course, very wrong.

### Revisiting the same bug, two years later

We use the old-school synchronous request-response model. Everything specific to the request lives in the execution stack of the request handler function OR in convenient global [context-aware variables](https://werkzeug.palletsprojects.com/en/stable/local/) like `frappe.db` which uses `LocalProxy` to magically return request specific database connection. It's like "thread locals" but instead "local" to a particular request. So it should not be very hard to safely use multi-threaded workers.

A few days ago I again started working on getting our codebase ready for multi-threaded deployments. Multi-threaded deployments are more memory efficient for us, by a factor of ~2x-4x. So the toil of dealing with these problems is worth it.

A colleague of mine asked if it's safe to deploy multi-threading configurations to old versions and eventually, that conversation went to _"what were the thread-safety fixes"_ we pushed after our last failed attempt.

I again saw this bug in the list and remembered how I never quite reproduced it or _knew_ if it was fixed.

Just last week I called out someone at work for saying _"I don't know why it was fixed"_ (for the record, I was right, that issue also wasn't fixed!).  It would be pretty bad for my _image_, if my two year old fix turned out to be invalid and exploded again in production. So yesterday at 9pm, I attempted reproducing this problem again.

### Reproducing the problem

I started by creating a new clean environment. Checked out the exact same version that showed this problem and created a completely untouched installation. My hypothesis was still that the original fix was valid, so let's reproduce it with the same assumptions.

I started by spawning a multithreaded web worker and bombarding it with two different requests. There's nothing unique about these requests, they just touch two different tables:
- `wrk -c5 -t1 -d500 http://{site}/api/resource/Role/Guest`
- `wrk -c5 -t1 -d500 http://{site}/api/resource/User/Guest`

Sure enough, in 4-5 minutes I saw the same error. That was so damn _satisfying_. Then I applied the original "fix" and again ran the same simulation and... it didn't fix the problem! <br>
_\*surprised Pikachu face\*_

### _Reliably_ reproducing the problem

If I wanted a real shot at root-causing this correctly, I needed a faster and repeatable way to reproduce this problem. Waiting for ~5 minutes is infeasible during debugging iterations, also how would I even know if the problem was fixed OR it just didn't occur for those 5 minutes?

Enter fuzzing. A few weeks ago I came across [cuzz](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/asplos277-pct.pdf) in a lecture about [software analysis techniques](https://rightingcode.github.io/index.html). Fuzzing for finding concurrency bugs essentially boils down to:
- Identify interesting points in program execution.
- Automatically introduce sleep statements to change [thread-schedules](/p/tip-concurrency-schedule).
- Rinse and repeat several times and there's a good chance you'll find a concurrency bug that's otherwise hard to spot.

I googled for any popular Python concurrency fuzzing utility but didn't find anything. I was too deep into this problem so I did the most lazy version of concurrency fuzzing by just manually slapping `time.sleep` in most likely places where it will work.

```python
def get_query(self, ...) -> Query:
    # Clean up state before each query
    self.tables = {}
    time.sleep(0.01)
      ☝️ most likely to trigger a bad schedule between creation and usage.
    ...
```

I tried a few different sleep values:
- 0.001s didn't work, maybe because it's smaller than `checkinterval` of Python's GIL which is 5ms?
- 1s and 0.1s didn't work, too big?
- 0.01 worked like a charm and I was able to reliably reproduce the error with just a burst of 3-5 requests instead of 1000s.

I replaced `wrk` with a small bash script to generate a burst of 5 requests. So now I had everything I needed for fast debugging cycles.

Note: This was obviously a fluke, you'll not always be this lucky. You'll need to write a more sophisticated fuzz test. You still need to attempt something in the _right direction_ to hit a fluke like this.

### Debugging

I tried a lot of random hypotheses, first I had to validate if `self.tables` variable was indeed the same for all requests. I printed the address of the tables object using `id(self.tables)` to see if it was changing between requests, and it was! So the original hypothesis of shared global mutable object was very quickly dismissed.

I then logged `id(self.tables)` every time it was modified. Every time the error occurred there were multiple mutations to the same object ID, which shouldn't happen as none of my test queries had any joins.

```
15:36:21 guni.1 | adding Sessions to 127409333222144
15:36:21 guni.1 | adding DocType to 127409332465856
15:36:21 guni.1 | adding DocField to 127409332455744 |
15:36:21 guni.1 | adding Sessions to 127409332455744 | 👈 Same object
15:36:21 guni.1 | adding DocType to 127409332718400
```

So even though every request had its own local state, _sometimes_ it was leaking into other requests. _The plot thickens._

I spent a couple of hours trying increasingly whackier hypotheses but none of them worked out. So I took a break and when I came back I decided to read all of the code related to this object from its creation to actual usage. This quickly resulted in a better hypothesis which was the actual root cause.

This is how the engine is initialized for each request:

```
        👇 local is that magical context-aware namespace
frappe.local.qb = get_query_builder(local.conf.db_type)
frappe.local.qb.engine = Engine()
```

When "Visually" inspecting the code, it looks fine, we are assigning the query builder and engine to the local namespace. But just "expanding" what it does makes the problem very clear:


```
             👇 qb is database specific CLASS e.g. MySQL or Postgres
frappe.local.qb = MySQLQueryBuilder if db_type is "MySQL" else PostgresQueryBuilder
frappe.local.qb.engine = Engine()
                 ☝️  engine is attached to a class definition, not an instance
         ☝️ The engine is NOT attached to the local namespace
```

Since class definitions are global, we were effectively storing the engine in a global dictionary even though visually it looked like we were storing it in the local namespace.

This bug eventually got fixed without us ever knowing about it when we refactored this code to make it stateless. So all I'll get out of this debugging effort is the satisfaction of really knowing what was going on.

### Takeaways

- "Fake Debugging" is harmful.
- If you answer "I don't know why" to the question of root cause, it means:
    - You don't want to spend effort to really find the root cause. This is rarely okay.
    - You simply don't have enough background knowledge required to even attempt root-causing the problem, this is fine! You get better at it by doing it more.
- It's ~~not possible~~ very hard to write a bug like this in a language like Rust, But we probably won't be able to ship products with nearly half a million lines of "business logic" like the [ones we currently have](https://frappe.io/products). I am yet to find a good compromise.
