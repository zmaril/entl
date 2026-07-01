# Entl — Purpose

Entl turns git repositories and forge activity into streaming data you can work with in any major language, with any major database.

## The Problem Everyone Has

I wanted to build a feature for [PowderMonkey](https://powdermonkey.dev) where the supervisor agent could look at all the recent merge conflicts that had happened, do some structured agent summarization magic, and produce proposals for refactoring tasks that would prevent similar merge conflicts from occurring in the future. Try launching a few waves of related tasks across independent agents, and you will soon know why this quickly became my focus.

Up until this point, I had been able to shell out to `gh` and use the GitHub API to do everything I wanted. And merge conflicts are easy to see, they are there in bright red on the PR and blocking the merge! What I didn't realize was that merge conflicts are only derived and are not actually recorded anywhere in git. Which makes sense, of course, you need two commits and a direction in order to declare whether a merge conflict would occur, there's a huge number of potential merge conflicts between arbitrary commits in any given repo, why store them all, but it was not something I had previously thought about. I needed a way to go back through time and find all the merge conflicts that had (likely) occurred during development.

Additionally, I had started thinking about the usefulness of whole repository analysis typically done in one-off tools like the [burndown charts from Hercules](https://github.com/src-d/hercules) and [the cityscapes-like visualization of CodeCharta](https://github.com/MaibornWolff/codecharta). I had many questions about codebases that I wanted to answer, that would help me improve the code being written by my agents. However, these proved to be difficult, with data wrangling and marshalling being a hard problem to solve, harder than just throwing an agent at a Python script and getting a good enough answer.

In short, I ran into the same problems with working with git as data that everyone else does.

## My Answer

Entl is my answer to these problems and more.

Entl is a modest library with modest goals:
1. Continually pull git repo and forge activity as data.
2. Stream this data as events that software can react to.
3. Store this data in any major database.
4. Perform common and custom analysis of this data.
5. Expose all of the above in every major programming language.

I think that all of the above is possible because of the following reasons:
1. I don't care about writing this data, only reading it, which greatly reduces the amount of work.
2. Rust has good libraries for accessing git and forge data.
3. Rust has good libraries for working with most popular databases.
4. Rust has good libraries for exposing functionality for other popular languages.
5. The shape of git and git forge data is pretty static at this point.
6. Entl as imagined is mostly glue code and coding agents have gotten pretty good at that.

Taken all together, the above goals seem pretty doable. The core of it is more than a day's worth of work, but less than a few months, and every part of it is immediately useful and necessary for [PowderMonkey](https://powdermonkey.dev).

## What's With All The Ducks?

You may have asked yourself, why is the library called Entl? More importantly, why are there ducklings in Tyrolean hats and dirndls all over the place? I had originally set out to load a git repo's full history into DuckDB and query it. So, I needed a duck related name. Ente means duck in German, Entchen means little duck or duckling, and Entl means little duck in Bavarian/Austrian dialect. Entl sounded like a good name for an ETL software library and [entl.dev](https://entl.dev) was available and that was enough to lock Entl in as the name. Simple as that!

## Conclusion

I hope you enjoy using Entl and that it becomes the de facto way of working with git and forge activity as data. I would like to solve this source of pain once and for all for everyone and would welcome your contributions towards that!
