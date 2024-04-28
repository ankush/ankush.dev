---
layout: post
title:  "Stop writing regexes to lint your code"
date:   2021-04-04

---


> The plural of regex is regrets.


The problem with regex-based linting tools is fundamentally you are reimplementing language's grammar in hacky manner and you'll inevitably find out edge cases.


Take this simple python code as an example.

```python
def add(a, b):
    return a + b
```

How would you match this code using regexes? Firstly regexes are a pain to work with multiple lines, secondly, python is not a regular language so you'll never be able to parse it with 100% guarantee using just regexes.


The most important thing to remember is: code is not a piece of text, it's a tree.
```
    function_definition
         /        \
      args        return
      /  \           |
     a    b       binary_op
                  /   |   \
                (+)   a    b
```

<center><figcaption>Mmmm, LISPy. λ</figcaption></center>

This code, when represented as tree removes lots of noise in the text that's irrelevant e.g. whitespace, newlines.


## Enter [Semgrep](https://github.com/returntocorp/semgrep) - "Semantic Grep"

- Utilizes tree representation of code instead of text for matching patterns.
- Started as `sgrep` at Facebook. c. 2009
- Now uses `tree-sitter`. Language agnostic syntax-tree generator from GitHub.
- Semgrep wants to position itself between dumb regexps and language aware linters.
- The pattern you define looks like original language source code


## Semgrep patterns

Two most basic primitive operators that will get you quite far using Semgrep are:

- things to ignore become --> `...`
- things to remember become --> `$VARIABLE`

Examples:

```python
print(...) # match any print statement

$A == $A   # match places where the same thing is compared with itself
```
## Finding similar bugs

Recently I came across a bug that was modifying objects after committing to the database. This change would not get committed to the database.

```python
def after_submit(self):
    # some irrelevant stuff that can be ignored
    self.status = "Submitted"
    # some more  irrelevant stuff
```

Naturally, you would want to check if the same issue is occurring in other places too. But there are thousands of instances where this method is used and writing code for one-off tasks is probably not worth the efforts.

Semgrep makes this task easy as well. You can copy-paste original buggy code, replace things to ignore with an ellipsis operator.

```python
def after_submit(self)
    ...
    self.$ATTR = ...
```

After you have written this rule, you might as well add it to CI so it's never missed in code reviews.

Finally, here's how you'd match the original example shown in first codeblock:

```python
def $FUNC($X, $Y):
  return $X + $Y
```

You can see this pattern in action on online editor: [https://semgrep.dev/s/O1ew/](https://semgrep.dev/s/O1ew/)

## Conclusion

The examples I shared are trivial but you can use them to build far complex patterns by composing individual patterns and combining them with boolean logic e.g. "match this pattern but not that pattern" and so on. The documentation is excellent for beginners, so I will just point to you to their [getting started](https://semgrep.dev/docs/getting-started/) page to begin using it.

