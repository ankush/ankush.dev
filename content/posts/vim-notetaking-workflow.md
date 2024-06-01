---
layout: post
title:  "Taking notes and managing tasks with Vim"
description: "My semi-finished VimWiki Frankenstein system"
date:   2020-11-09
---


I have been working on a system for notetaking and basic task management. At this point I feel I have perfected the notetaking part but need to work more on the task management aspect of the system. So I though I should document my thoughts.

### Requirements for designing my current system.

- **Portability.** No proprietary or vendor specific data formats allowed.
- **Speed > Quality.** Should be able to take notes and edit them at speed of thought. Aligns with Vim philosophy.
- **Searchability > Quality.** Notes Should be searchable.
- 10 seconds to capture a thought that's about one line long.
- **Visibility** - Make it VERY easy to decide what needs to be done next.


### Implementation
#### Plain text markdown files.
What is more portable than plain text files? Add some basic markdown support and now you've got 90% of what you need to take quick notes. The editor of choice for me was Neovim.

#### Speed
Vim is designed for super fast text editing. VimWiki plugin takes care of dynamically creating new files and linking them.
VimWiki also has a diary feature which I've utilized to create daily files. I use a calendar plugin to jump to future/past dates.

I have setup my Window manager such that I can bring my today's diary page up with single button click. Pressing F8 on my system brings up today's diary page as a XMonad scratchpad with planned next actions, a daily checklist of things and a dedicated space to jot down incoming tasks / information. Pressing F8 again moves the window to background.

```bash
# launch command
terminator --title "vimwiki" -e "nvim -c 'let g:startify_disable_at_vimenter = 1' +VimwikiMakeDiaryNote"
```

I also use a template for diary. Vimwiki does not support it by default to my knowledge but it is very easy to setup using vim autocmd. Whenever a new file is opened with location pattern specified, vim will fetch the template and paste it into new file.

```vim
autocmd BufNewFile */wiki/diary/[0-9]*.md :read ~/wiki/diary/templates/template.md
```

I have also memorized typical vimwiki keyboard shortcuts. You should definitely consider doing and reading the manual that if you intend to use it for long time. `:h vimwiki`

#### Searchability
I use [fzf](https://github.com/junegunn/fzf.vim) for fuzzy finding along with [ripgrep](https://github.com/BurntSushi/ripgrep) in Vim. I tend to vaguely remember where I need to jump to. A fuzzy finder like FZF makes this job very easy for me. This made even more sense since I use it a lot while programming too.

#### Visibility
I use bunch of shell scripts to extract tasks from my files. One such example is using bash script to extract checklist form today's diary page and show it as conky widget on desktop. So whenever I've closed all windows, I will clearly see what's coming next. Following snippet gets all the lines with unchecked boxes from today's diary entry.

```bash
#!/bin/bash

rg '\[ \]' "$HOME/wiki/diary/$(date -I'date').md" | cut -c2-
```


## Conclusion
I have created a Frankenstein monster of a system for basic task management and note taking. It uses Neovim, XMonad, vimwiki and calendar plugin, multiple hacked together shell scripts to achieve end result I wanted. It might appear as too complicated and you might think "I don't need this" and you're probably right. You need to come up something that works for you, but I do hope this post gave you some idea about it.

You can always checkout my full config in my [dotfiles](https://github.com/ankush/dotfiles/)
