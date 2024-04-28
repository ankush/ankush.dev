---
layout: post
title:  "Parsing 140 gigabytes of chess games without compute clusters"
subtitle: "or why you should learn unix shell tools"
date:   2021-08-28
---

Ok, "parsing" might be an exaggeration here but hear me out.

A few months ago, for a college project I had to analyze "some" datasets and to go aBoVe and BeYoNd* I picked up a chess dataset from lichess. Lichess [provides a dump of all games](https://database.lichess.org/) played in `pgn` format grouped by months. I decided to start with one month's data which was 17.5 GB bz2 compressed and ~140GB uncompressed.

If you've never heard of PGN format, here's a quick primer: it's a text file that contains information about a chess game, the outcome of the game and chess moves in algebraic notation. Metadata can also be added. Following is an example of one game from ~70 million in the dataset. Text in round bracket is explanation added by me.

```
[Event "Rated Bullet tournament https://lichess.org/tournament/yc1WW2Ox"]  (event type and link)
  [Site "https://lichess.org/PpwPOZMq"]        (unique game link)
  [White "Abbot"]                              (username of white player)
  [Black "Costello"]                           (username of black player)
  [Result "0-1"]                               (Result of game - "W-B")
  [UTCDate "2017.04.01"]                       (Date of game)
  [UTCTime "11:32:01"]                         (Time of game)
  [WhiteElo "2100"]                            (ELO of  white player)
  [BlackElo "2000"]                            (ELO of black player)
  [WhiteRatingDiff "-4"]                       (White rating change)
  [BlackRatingDiff "+1"]                       (Black rating change)
  [WhiteTitle "FM"]                            (Optional title)
  [ECO "B30"]                                  (ECO code for chess opening)
  [Opening "Sicilian Defense: Old Sicilian"]   (Name and variant of chess opening)
  [TimeControl "300+0"]                        (Time control for game)
  [Termination "Time forfeit"]                 (How game terminated)

  1. e4 { [%eval 0.17] [%clk 0:00:30] } 1... c5 { [%eval 0.19] [%clk 0:00:30] }
  2. Nf3 { [%eval 0.25] [%clk 0:00:29] } 2... Nc6 { [%eval 0.33] [%clk 0:00:30] }
  3. Bc4 { [%eval -0.13] [%clk 0:00:28] } 3... e6 { [%eval -0.04] [%clk 0:00:30] }
  ... (more game moves)
```

Being the python n00b I am, I quickly just searched  "parse PGN file in python" and found a library that parses PGN files. In 10 minutes I wrote a function to import all data in memory and create a pandas dataframe for further analysis... and it never finished executing. So I added some print statements to track progress and found that it's parsing a whopping 500 games/second. So it would take me **~39 hours** just to get 70 million games parsed.

So instead of relying on full-fledged parser to give me a game object, I transformed the data using Unix shell tools like `grep`/`rg`, `sed`, `awk`. These are all designed for one task and they do that one task well - text manipulation.

My first realization was that size of this massive data is just too much and I will never be able to load it in memory locally. Which meant I would have to use some out-of-core computation library on top of Pandas or rent a server from daddy Google. I was only interested in capturing summary statistics, so all game moves were of no use to me. I began transforming data to eliminate the information I didn't need and "compress" information that I did require.

Most of these "transformations" read the entire file but only took few minutes to complete. Most of these "transformations" are also hacky and some Linux hacker will surely figure out a much better alternative, but these *worked* and worked much better than the alternative, so I didn't bother optimizing.

### 1. Get rid of moves.

```
processed=lichess_db_standard_rated_2020-10_processed.pgn

# remove lines starting with 1 which are game logs
# remove lines containing unnecessary fields for analysis. To speed up analysis
rg -v "^(1|\[(ECO|Time|WhiteR|BlackR|UTCTi|Site|Result))" lichess_db_standard_rated_2020-10.pgn > $processed
```

### 2. Capture interesting fields and put them in a new file.


```
# can be optimized to capture key as filename from key:value pairs
rg -o '\[Event "Rated ([a-zA-Z]+) .*"\]' -r '$1' $processed > event.txt
rg -o '\[White "(.*)"\]' -r '$1' $processed > white.txt
rg -o '\[Black "(.*)"\]' -r '$1' $processed > black.txt
rg -o '\[Result "(.*)"\]' -r '$1' $processed > result.txt
rg -o '\[UTCDate "2020\.10\.(.*)"\]' -r '$1' $processed > utcdate.txt
rg -o '\[WhiteElo "(.*)"\]' -r '$1' $processed > whitelo.txt
rg -o '\[BlackElo "(.*)"\]' -r '$1' $processed > blackelo.txt
rg -o '\[Opening "(.*)"\]' -r '$1' $processed > opening.txt
rg -o '\[Termination "(.*)"\]' -r '$1' $processed > termination.txt
```

### 3. Convert other Textual info to fake enums to reduce filesize.

```
# change results to: *, (W)hite, (B)lack, (D)raw
sed -i -e 's/1-0/W/' -e 's/0-1/B/' -e 's/1\/2-1\/2/D/' result.txt

# clean up Termination
# (A) Abandoned
# (N) Normal
# (R) Rules infraction
# (T) Time forfeit
# (U) Unterminated

cut -c 1 termination.txt > termination_cleaned.txt

# Clean up event type
# (Bl)itz
# (Bu)llet
# (Cl)assical
# (Co)rrespondence
# (Ra)pid
# (Ul)traBullet
cut -c -2 event.txt > event_cleaned.txt
```

### 4. Chess openings have small variants, I was not really interested in those, so I stripped them out.
```
# clean up openings
sed -i -E 's/(:| Accepted| Declined|Refused|,).*$//' opening.txt
```

### 5. Finally, join all separate text files into final CSV

```
# join files into a single csv
paste -d, utcdate.txt event.txt white.txt black.txt whitelo.txt blackelo.txt result.txt termination.txt opening.txt > lichess.csv
```

This final file turned out be ~3.8 GB which meant I could simply load the entire file in memory. This whole process probably took ~2 hours of compute time and ~6 hours of fiddling around the data and final dataset looked like this:

```
day,eventtype,white_player,black_player,white_elo,black_elo,result,termination,opening
01,Bl,capigmc2018,Magojey,1979,1956,B,N,Caro-Kann Defense
01,Ra,MFUNES,ActionMel,1156,1138,B,N,King's Pawn Game
01,Ra,NovemberV,GhostClub,1697,1703,B,N,Queen's Pawn Game
01,Ra,Cost2Be3,mariocbsf,1254,1262,W,N,Bishop's Opening
01,Bl,erivambrito,Andreiavfs1982,1815,857,W,N,Italian Game
01,Bl,jonatan-jaguaretama,Joaomarcelo08,1850,1377,D,T,Caro-Kann Defense
01,Bl,Gadiel_Fernandess,Gusdf333,1425,1877,B,N,Caro-Kann Defense
01,Bl,Felipevfs051,WagnerAlexandre,1271,1839,B,N,Italian Game
01,Bl,Estolaski,kimuraal,1600,1895,W,T,French Defense
01,Bl,Diegoj89,Ernanemmc,1877,1447,B,N,Queen's Gambit
```

## Conclusion:

Unix shell tools build for text processing have been around for a really long time and they've stood the test of time. So if you're processing structured textual data, you should definitely consider them before starting a dIsTriButeD coMpUte CluSter.

If you want to learn more about this kinda "data wrangling" with shell tools I highly recommend a lecture from MIT's course "Missing Semester": [Data Wrangling](https://missing.csail.mit.edu/2020/data-wrangling/)
