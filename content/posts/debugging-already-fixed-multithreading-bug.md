---
layout: post
title:  'Debugging a Two Year Old "Fake Debugged" Multi-Threading Bug'
description: "Fake Debugging Considered Harmful."
date: 2024-12-25
discussions:
---


> "અધૂરું જ્ઞાન હોવા કરતા કશું ન જાણવું વધુ સારું છે."
>
> "It's better to know nothing than to have half-knowledge." (translation)  <br>
>
> -- WhatsApp forward "wiSdoM"

This is a story of how I "fake debugged" one of the worst multi-threading bug we saw at work around 2 years ago and then how I _really_ debugged it yesterday.


### First signs of trouble

Multiple users at work reported getting `column "name" is ambiguous` errors randomly while doing anything. This is an error reported by MySQL when you use a column name without specifying which table it belongs to and it's present in more than 1 table in the query.

Some context:
- I work on [Frappe Framework](https://frappe.io/framework) which is "full stack" web framework like Django/Ruby On Rails but specifically designed for writing business apps like [ERPNext](https://erpnext.com/). You don't need to know much about these to read the rest of the post.
- Business apps tend to have a lot of queries, we have **thousands** of unique queries in our codebase.
- This bug is in unsafe implementation of our SQL query builder abstraction.

### First encounter and a temporary fix

First, I started with looking at error logs to see if there is any common pattern. All of these errors were caused by weird queries that no one would ever write themselves.

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

"Pick List" is document for warehouse operators to work on order fulfilment. Why would anyone ever join OAuth table with Pick List table? This query made no sense.

Whenever I am debugging crazy issues like these I ensure leaving breadcrumbs on ticket so we can revisit it later some day. This was my first internal comment on ticket:

> "Something very bad is going on. Random queries have joins with random tables which don't make any sense. Probably a QB (Query Builder) bug."


Then I remembered we had experimentally enabled multi-threaded workers for few users. This was in era before we started using Sentry for centralized error reporting. So I had to check them manually to see if all of them are using multi-threaded web workers and they were.

So I quickly rolled back the configuration change for them and monitored errors for a while. Error logs stopped completely. So, that was a very good hint that this was indeed a multi-threading bug.

### First (incorrect) diagnosis

After rolling back the change, now I had enough time to investigate it in depth. We use Python, so thread-safety issues mostly boil down to:

1. Mutable Global Variables
2. Mutable Class Attributes (which are shared with all instances)

I started reading query builder code and sure enough, I found **exactly** what I was looking for:

```python
class Engine:  # Query building "engine"
    tables = {}   👈 shared mutable state.
    ...
```

Query building engine had a shared mutable class attribute called `tables` and this attribute is responsible for storing all the joins to be made while constructing final query. So I just assumed it must be this, since ALL the signs point to it?

1. A global shared mutable object
2. The object is responsible for joining queries and we have problem with random joins.

I changed the code to remove shared attribute and use instance attribute instead.

```python
class Engine:
    def __init__(self):
        self.tables = {}
    ...
```

I was never quite able to "reproduce" this issue in a clean environment and no way I was gonna ask users to face this again, so I just assume we solved it and moved on.

Since we ALSO rolled back the multi-threading configuration change this problem never resurfaced.

### Doubts about the first diagnosis

While discussing this fix with other enigneers (since a lot of us got random ticket related to this in different products) , A colleague of mine pointed out that even though there is this shared mutable attribute, it's always overridden by instance attribute in code:

```python
def get_query(self, ...) -> Query:
    # Clean up state before each query
    self.tables = {}
```

My memory is bit hazy but if I recall correctly we didn't investigate it further thinking this is some side effect of having instance attribute shadowing a class attribute with same name. Since I was not able to reproduce the problem in first place, how would I even go about validating this? Spoiler: I was of course, very wrong.

### Revisiting same bug, after two years

Few days ago I again started working on getting our codebase ready for multi-threaded implementation. Multi-threaded deployments are more memory efficient for us, by factor of ~2x-4x! So the toil of dealing with these problem is worth it.

A colleague of mine asked if it's safe to deploy these changes to old versions and eventually the conversation went to "what were the thread-safety fixes" we pushed after our last stint.

Some additional context will help here: we don't have many problems with multi-threading bugs because our web worker use the old school synchronous request-response model. Everything specific to request lives in execution stack of request handler function OR in convenient global context-aware variables like `frappe.db` which uses `LocalProxy` to magically return request specific version of "global" variables like a connection to the database. This is similar to Flask's `g` object.

I again saw this bug in the list and remembered how I never quite reproduced it or knew if it was fixed.

Just last week I lectured someone at work for saying _"I don't know why it was fixed"_ (for the record, I was right, it wasn't fixed).  It would be pretty bad for my image if my 2 years old fix turned out to be invalid and exploded again in production. So yesterday at 9pm, I decided I am gonna attempt reproducing this problem again.

### Reproducing the problem

I started by creating a new clean environment. Checked out the exact same version that showed this problem and created a completely untouched installation. My hypothesis was still that original fix was valid, so let's reproduce it with same assumptions.

I started by spawning multi-threading web workers and bombarding it with two different requests. There's nothing unique about these requests, they just touch two different tables:
- `wrk -c5 -t1 -d500 http://site/api/resource/Role/Guest`
- `wrk -c5 -t1 -d500 http://site/api/resource/User/Guest`

Sure enough, in 4-5 minutes I saw the same error. That was so damn _satisfying_. Then I applied the original "fix" and again ran the same simulation and... <br> \*surprise pikachu face\* it didn't fix the problem!

### _Reliably_ reproducing the issue

If I wanted a real shot at root-causing this correctly, I needed a faster and repeatable way to reproduce this problem. Waiting for ~5 minutes is just infeasible during debugging iterations, also how would I even know if the problem was fixed OR it just didn't occur for those 5 minutes?


Enter fuzzing. Few weeks ago I came across [cuzz](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/asplos277-pct.pdf) from a lecture at OMSCS about software analysis techniques. Fuzzing for detecting concurrency bugs essentially boils down to:
- Identify interesting program points
- Introduce sleep statements to change [thread-schedules](/p/tip-concurrency-schedule)

XXX: EXPAND

I googled for any popular concurrency fuzzing utility but didn't find anything simple. I was too deep into this problem so I did the most lazy version of concurrency fuzzing by just manually slapping `time.sleep` in most likely place where it will work.

```python
def get_query(self, ...) -> Query:
    # Clean up state before each query
    self.tables = {}
    time.sleep(0.01)
      ☝️ most likely to trigger bad schedule between creation and usage.
    ...
```

I tried few different sleep values:
- 0.001s didn't work, maybe because it's smaller than `checkinterval` of Python's GIL which is 5ms?
- 1s and 0.1s didn't work, too big?
- 0.01 worked like charm and I was able to reliable reproduce the error with just burst of 3-5 requests.

I replaced `wrk` with a small bash script to generate burst of 5 requests. So now I had everything I needed for fast debugging cycles.

Note: This was obviously a fluke, you'll not always be this lucky and need to write a sophisticated fuzz test. You still need to attempt something in right direction to hit a fluke like this.

### Debugging

I tried a lot of random hypothesis, first I had to validate if `self.tables` variable was indeed same. I printed address of tables object using `id(self.tables)` to see if it was changing between requests, and it was! So the original hypothesis of shared global mutable object was very quickly dismissed.

I then logged `id(self.tables)` and its content every time it was modified. Every time the error occurred there were multiple mutation to same object ID.

```
15:36:21 guni.1 | adding Sessions to 127409333222144
15:36:21 guni.1 | adding DocType to 127409332465856
15:36:21 guni.1 | adding DocField to 127409332455744 |
15:36:21 guni.1 | adding Sessions to 127409332455744 | 👈 Same object
15:36:21 guni.1 | adding DocType to 127409332718400
```

So even though every request had its own local state, _sometimes_ it was leaking into other requests. The plot just got a lot more thick.

I spent couple of hours trying increasingly wackier hypotheses but none of them worked out. So I took a break and when I came back I decided to read all of the code related to this object from its creation to actual usage. This quickly resulted in a better hypothesis which was the actual root cause. This is how engine is initialized for each request:

```
        👇 local is a that magical context-aware namespace.
frappe.local.qb = get_query_builder(local.conf.db_type)
frappe.local.qb.engine = Engine()
```

When "Visually" inspecting the code, it looks fine, we are assigning query builder and engine to local namespace. But just "expanding" what it does makes the problem very clear:


```
             👇 qb is database specific CLASS e.g. MySQL or Postgres
frappe.local.qb = MySQLQueryBuilder if db_type is "MySQL" else PostgresQueryBuilder
frappe.local.qb.engine = Engine()
                 ☝️  engine is attached to a class definition, not instance.
```

Since class definitions are global, we were effectively storing engine in a global dictionary even though visually it looked like were storing it local namespace.

This bug eventually got fixed without us ever knowing about it when we refactored this code to make it stateless. So all I'll get out of this debugging effort is satisfaction of really knowing what was going on.

### Takeaways

- "Fake Debugging" is harmful.
- If you answer "I don't know why" to the question of root cause, it means:
    - You don't want to spend effort to really find the root cause. This is rarely okay.
    - You simply don't have enough background knowledge required to even attempt root-causing the problem, this is fine! You get better at it by doing it more.
- There are no "deep problems", it's question of you deep _you_ want to go into a problem.
- This bug wouldn't even exist in Rust! But we probably won't be able to ship products with nearly half a million lines of business logic like the [ones we currently have](https://frappe.io/products). I am yet to find a good compromise.
