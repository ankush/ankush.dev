---
layout: post
title:  "Solving Concurrency Bugs Using Schedules and Imagination"
subtitle: "Race conditions are hard, debugging them without right approach is even harder."
date:   2024-05-29
---


As web developers, we usually don't have to deal with concurrency bugs much. Everything we need is neatly abstracted and the databases we use carry us for the most part. However, business apps don't have that luxury, which are always transactional in nature and have long-running transactions.

Once you hit a certain number of users, every scenario you think is "rare" will start occurring. You can choose to ignore and maybe manually patch it but that won't scale in the long term. You have to patch concurrency bugs, regardless of how rare they are.

### Why concurrency bugs are not escapable

Databases provide [ACID](https://en.wikipedia.org/wiki/ACID) properties, which are foundational in building web apps without worrying about all the hard parts. "**I**" in ACID stands for isolation. However, that isolation is usually provided at varying levels. Strong isolation levels come at the steep cost of losing parallelism and performance. So we have to use weaker isolations and rely on *some* pessimistic locking at the application layer.


### Why debugging concurrency issues is hard

Traditional debugging methods of printing values at certain step in execution or stepping through execution using debuggers are simply not efficient for debugging concurrency bugs. This is mainly because of the nature of concurrent transactions where there can be many possible interleaving of operations. Concurrency errors occur under very specific and rare conditions and to reproduce them you need to know the exact interleaving.

I've had this conversation multiple times:

> I can't reproduce it, that can't happen!
>
> But it **happened**?

I'm not implying it's impossible. It is possible to write a simulation, add some logging and keep running it until the desired anomaly occurs. The problem is, in most cases, we don't even know what to simulate. This is also why tools like TLA+ are out of the picture.

### Schedule Diagrams

[Schedule diagrams](https://en.m.wikipedia.org/wiki/Database_transaction_schedule) are simple tables with monotonically increasing timestamps as index and two or more transactions as columns. The following example shows how you can lose a write in "REPEATABLE READ" isolation level.


| Timestamp | UserA    | User B   |
| :-:       | :-:      | :-:      |
| 1         | begin    |          |
| 2         | read(x)  | begin    |
| 3         | write(x) | read(x)  |
| 4         |          | write(x) |
| 5         | commit   | stalled  |
| 6         |          | commit   |


Schedule diagrams are easy to build when you just have one variable and 2 known transaction, reality is however rarely as simple. So how do you go about building them in practice?


#### 1. Identify transactions - read the logs


Most business apps will have some kind of `timestamp` for identifying when a record was last modified. When some anomaly occurs you might be able to construct some kind of interleaved execution considering these breadcrumbs of execution.

If application level logging isn't sufficient then you might have to read the database' write logs (e.g. Binary log on MySQL) Those will have exact database write with timestamps. While this isn't enough to figure out complete interleaving of transactions, it's a good start and you can use imagination to fill in the blanks.


#### 2. Prepare a draft schedule diagram

Identify all major reads and writes in your transactions. You need to specifically look at any writes that might have dependent reads. E.g. `A = X + Y` means you first read `X` and `Y` and then updated `A`.

To do this, you should read the logs and also the code that produces them.

#### 3. Prepare hypothesis, test and repeat

You should try to prepare an interleaving that's most likely representing your anomaly. If you can build one, then great! Now all you need to do is sprinkle a small amount of pessimistic locking or some rearranging of code to avoid anomaly in lock-free manner.

If you can't build an interleaving that can produce your anomaly, then most likely you're still missing something. Put your imagination to use:

- Am I missing some write?
- What rearrangement of these operations can produce anomaly?
- What kind of delays introduced between any operation can produce anomaly?
- Am I missing a lock? Am I releasing locks too early?

### Examples

This model is easy to remember but hard to implement, so let's look at a few good examples of applying this in practice.

#### 1. Debugging a lost update

This example consist of users assigning themselves to a task. When user is assigned they do two operations:

1. Create assignment record
2. Update list of assignees in task

This roughly translates to:

```sql
insert into assignments values (...)
select * from assignments where task = 123;
update task.assignees = [...];
```

When two users simultaneously assign themselves to the same task, the assignees list was missing one value. Let's build a schedule just using our imagination.


| Timestamp | UserA                         | User B                      |
| :-:       | :-:                           | :-:                         |
| 1         | begin                         |                             |
| 2         | insert_assignment(a)          | begin                       |
| 3         | read_all() -> [a]             | insert_assignment(b)        |
| 4         | update_assignee([a])          | read_all -> [b]             |
| 5         | commit                        | update_assignee([b])        |
| 6         |                               | commit                      |

As you can see because of "repeatable read", both users can't see each other's assignments. Only way to resolve this is by adding pessimistic locking at app layer using `for update` while reading list of current assignees. Here's how it will look like after locking:


```diff
- select * from assignments where task = 123;
+ select * from assignments where task = 123 for update;
```


| Timestamp | UserA                            | User B                              |
| :-:       | :-:                              | :-:                                 |
| 1         | begin                            |                                     |
| 2         | insert_assignment(a)             | begin                               |
| 3         | ex_read_all() -> [a]             | insert_assignment(b)                |
| 4         | update_assignee([a])             | ex_read_all() -> ...                |
| 5         | commit                           | **stalled**                         |
| 6         |                                  | **ex_read_all() -> [a, b]**         |
| 7         |                                  | update_assignee([a, b])             |
| 8         |                                  | commit                              |


"FOR UPDATE" gives you exclusive read lock on rows it reads AND non-existing rows it can potentially read by [locking gaps](https://dev.mysql.com/doc/refman/8.4/en/innodb-locking.html#innodb-gap-locks) between data. This way you can be sure that no possible interleaving of these two transactions will ever cause invalid cache update.


Ref: https://github.com/frappe/frappe/pull/26592

#### 2. Debugging stale cache

Let's look at something that involves DB and a separate cache like Redis. This method of drawing schedules still helps.

A naive way to implement caching over a document would look something like this:

```
def get_record(id):
    record = db.sql(f"select * from table where id={id}")
    return record

def get_cached_record(id):
    cached_record = cache.get(id)  # Get from Cache
    if not cached_record:          # Cache miss
        record = get_record(id)    # Get from DB
        cache.set(id, record)      # Store in cache
    return record

def update_record(id, values):
    db.sql(f"update table where ...")  # Update in DB
    cache.delete(id)                   # Evict cache
```

At surface, this invalidation implementation seems reasonable. But what happens when this document is frequently updated and read? Is there any possible interleaving where invalidation will be incorrect?

Applying some imagination, we can come up with an interleaving that will cause stale data to be stored in cache:

| Timestamp | UserA                            | User B                              |
| :-:       | :-:                              | :-:                                 |
| 1         | begin                            |                                     |
| 2         | read(X)                          |                                     |
| 3         | update(x)                        |                                     |
| 4         | clear_cache(x)                   | begin                               |
| 5         |                                  | get_cache(x) -> miss                |
| 6         |                                  | read(x)                             |
| 7         | commit                           | **cache(x)**                        |
| 8         |                                  | commit                              |

Repeatable Read strikes again. Because there is short delay between updating and committing changes to DB, some other transaction can read old data and store it in cache.

Solution: Eviction has to happen _AFTER_ we have committed the changes. This way, cache will read stale values from DB.


| Timestamp | UserA                            | User B                              |
| :-:       | :-:                              | :-:                                 |
| 1         | begin                            |                                     |
| 2         | read(X)                          |                                     |
| 3         | update(x)                        | begin                               |
| 4         |                                  | get_cache(x) -> hit                 |
| 5         |                                  | commit                                  |
| 6         | commit                           |                                         |
| 7         | **clear_cache(x)**               |                                         |



Ref: https://github.com/frappe/frappe/pull/21216


#### 3. Debugging double execution of exclusive operation


In this example we look at freezing a record. This is a common operation in business applications. Obviously, once a record is frozen it should not be modified and no two transactions should be able to freeze a record at same time.

By default this operation already had sufficient pessimistic locking and yet I encountered a case where a record was frozen twice.

Let's try to build a schedule for two user trying to freeze a record:

| Timestamp | UserA     | User B            |
| :-:       | :-:       | :-:               |
| 1         | begin     |                   |
| 2         | lock(x)   | begin             |
| 3         | freeze(x) | lock(x)           |
| 4         | commit    | stalled           |
| 5         |           | fail and rollback |


At first glance, there is no way two users can freeze a record. So we need to again put our imagination at work and figure out ways in which case pessimistic locking can fail.

In databases doing typical [Two Phase Locking](https://en.wikipedia.org/wiki/Two-phase_locking), locks are held until you commit the transaction, so what if the transaction is getting committed? `freeze(x)` is a fairly complicated process and if someone commits transaction during that process, the lock is immediately released and not re-acquired. Soon after forming this hypothesis, I found a transaction commit deep inside the call for `freeze()`.

Here is how that interleaving would look like.


| Timestamp | UserA              | User B        |
| :-:       | :-:                | :-:           |
| 1         | begin              |               |
| 2         | lock(x)            | begin         |
| 3         | freeze(x)          | lock(x)       |
| 4         | **freeze->commit** |               |
| 5         | freeze->update(x)  | lock acquired |
| 6         | commit             | freeze(x)     |
| 7         |                    | commit        |

Ref: https://github.com/frappe/frappe/pull/25256


### Conclusion

I have found that this idea of drawing a schedule and imagining bad interleaving works with any kind of concurrency bug for me. I hope you found this useful too.
