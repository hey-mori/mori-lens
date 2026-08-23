# What a profile changes, 2026-08-18

The roadmap's M5 has said *not started: the developer and exam profiles* since
the milestone was written. This is the measurement that goes with building them.

A profile is one line added to the deep system prompt. A line in a prompt that
sounds better is not better until it is run: this crate carries
`body_for_with_extra` for exactly that reason, and
[`does hardening the fence help`](2026-08-18-does-hardening-the-fence-help.md)
measured a well-meant prompt line that made an injection twice as likely.

Two sentences, three profiles, six runs each, `qwen2.5:7b`.

## The claims, and whether they held

```
                          품사 dropped   구조 present   참고 present
reader (unchanged)        0 of 12        7 of 12       0 of 12
developer                 12 of 12       1 of 12       1 of 12
exam                      0 of 12        11 of 12      0 of 12
```

**Developer drops the part of speech every single time**, twelve of twelve
across two sentences and two samples. It drops the clause structure eleven times
of twelve.

**Exam fills the clause structure eleven times of twelve**, against seven for
the profile that shipped. That is an improvement and not an absolute one, and
the difference is worth stating as 11 against 7 rather than as "exam explains
the structure".

## What the cards actually said

```
reader     원형=stale  품사=형용사  이 문장에서=애플리케이션이 이미 삭제했지만 트리에서는
           여전히 존재하는 요소  번역에서=오래된  구조=동명사 + 서술절
developer  원형=stale  이 문장에서=애플리케이션에서 삭제되었지만 웹 페이지 트리에서는
           여전히 존재하는 요소  번역에서=오래된
exam       원형=stale  품사=명사  이 문장에서=애플리케이션에서 삭제되었지만 트리에서는
           여전히 존재하는 요소  번역에서=오래된  구조=가정문: 주어 + 서술어
```

Three lines against five is most of a card back, on a surface whose whole
constraint is 300 logical pixels.

## The half that did not work

**`참고` is filled once in twenty-four runs.** The developer line says to spend
it on the jargon: an acronym, an API term, an ordinary word used in a technical
sense. `stale` and `lease` are both exactly that and the note came back empty
anyway.

The instruction it competes with is the shipped one, which says most sentences
need no note and an empty string is the expected answer. That sentence is doing
its job and the profile line is not strong enough to move it. **Left as it is
rather than escalated**, because the alternative is a card that always has a
note whether or not there is one to make, and the shipped instruction exists
because that was the failure it was written against.

## And it is not faster

```
reader     5192ms  6419ms
developer  5383ms  6073ms
exam       5319ms  6088ms
```

Two fewer rows did not buy time. The rows a profile drops are short ones, and
the answer is dominated by the sense line, which every profile asks for. Worth
recording because the opposite was the natural assumption.

## What is not measured

- **Two sentences, one model, one direction, twelve runs a profile.** Both
  sentences are technical English, which is the developer profile's home ground
  and not the exam profile's.
- **Whether an examination reader is better served.** The exam profile fills
  `구조` more often. Nothing here says the contents are right, and the
  `구조=가정문` above is describing a conditional in a sentence that has none.
- **The reader who set it.** No settings file was read in this run. The profile
  was passed directly, which is the layer under the one a person touches.
- **Whether the note line could be made to work** with a wording that does not
  cost every other sentence a spurious note.
