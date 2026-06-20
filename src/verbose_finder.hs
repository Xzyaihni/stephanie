import Data.Maybe
import Data.List


data Direction = DUp | DDown | DLeft | DRight deriving (Eq, Show);

parseDirection :: String -> (Direction, String)
parseDirection ('U':'p':rest) = (DUp, rest)
parseDirection ('D':'o':'w':'n':rest) = (DDown, rest)
parseDirection ('L':'e':'f':'t':rest) = (DLeft, rest)
parseDirection ('R':'i':'g':'h':'t':rest) = (DRight, rest)

data Pos2 = Pos2 Int Int deriving (Eq, Show);

parsePos :: String -> (Pos2, String)
parsePos s = let (firstNumber, rest) = span (\c -> c /= ',') $ tail s
             in let (secondNumber, restSecond) = span (\c -> c /= ']') $ tail rest
                in ((Pos2 (read firstNumber) (read secondNumber)), tail restSecond)

data State = State String;

instance Show State where
      show (State s) = s

parseStates :: String -> ([State], String)
parseStates s = let (state, rest) = span (\c -> c /= ',' && c /= ']') $ tail s
                in if (head rest) == ']'
                      then ([(State state)], rest)
                      else let (parsedStates, lastRest) = (parseStates (tail rest))
                           in (((State state) : parsedStates), lastRest)

data ConstrainPre = ConstrainPre Direction (Pos2, [State]) (Pos2, [State]) deriving Show;
data ConstrainPost = ConstrainPost [State] [State] deriving Show;

parsePre :: String -> ConstrainPre
parsePre s = let (direction, rest) = parseDirection s
             in let (posA, restPosA) = parsePos $ fromJust $ stripPrefix " constraining " rest
                in let (posB, restPosB) = parsePos $ fromJust $ stripPrefix " against " restPosA
                   in let (statesA, restStatesA) = parseStates $ drop 2 restPosB
                      in let (statesB, _) = parseStates $ drop 4 restStatesA
                         in ConstrainPre direction (posA, statesA) (posB, statesB)

parsePost :: String -> ConstrainPost
parsePost s = let (statesA, rest) = parseStates $ fromJust $ stripPrefix "after: " s
              in let (statesB, _) = parseStates $ drop 4 rest
                 in ConstrainPost statesA statesB

parsePair :: (String, String) -> (ConstrainPre, ConstrainPost)
parsePair (pre, post) = (parsePre pre, parsePost post)

linePairs :: [String] -> [(ConstrainPre, ConstrainPost)]
linePairs [] = []
linePairs (x:[]) = []
linePairs (x:xs) = if isInfixOf " constraining " x
                      then (parsePair (x, head xs)) : (linePairs (tail xs))
                      else linePairs xs

verbosePairs :: String -> IO [(ConstrainPre, ConstrainPost)]
verbosePairs path = fmap (\s -> linePairs $ lines s) $ readFile path

filterWithPos :: Pos2 -> [(ConstrainPre, ConstrainPost)] -> [(ConstrainPre, ConstrainPost)]
filterWithPos needle = filter (\((ConstrainPre _dir (posA, _) (posB, _)), _post) -> posA == needle || posB == needle)

hasState :: String -> [State] -> Bool
hasState needle = any (\(State state) -> needle == state)

findWhereLosesState :: Pos2 -> String -> [(ConstrainPre, ConstrainPost)] -> (ConstrainPre, ConstrainPost)
findWhereLosesState needle s xs = head
      $ filter (\((ConstrainPre _dir (posA, _) _), (ConstrainPost statesA statesB)) -> if posA == needle
                                                                                   then not (hasState s statesA)
                                                                                   else not (hasState s statesB))
      $ filterWithPos needle xs
