import Numeric
import Data.Char
import Data.Maybe
import Data.List


hexChars :: [Char]
hexChars = (map intToDigit [0..9]) ++ ['a'..'f']

hexToDigit :: Char -> Int
hexToDigit b = fromJust $ elemIndex b hexChars

leftPad :: Int -> Char -> String -> String
leftPad total c s = if (length s) < total then (leftPad total c (c : s)) else s

byteFromNibs :: Char -> Char -> Int
byteFromNibs a b = ((hexToDigit a) * (2 ^ 4)) + hexToDigit b

fromHexColor :: String -> [Int]
fromHexColor (r0 : r1 : g0 : g1 : b0 : b1 : []) = [byteFromNibs r0 r1, byteFromNibs g0 g1, byteFromNibs b0 b1]

toHexColor :: [Int] -> String
toHexColor c = leftPad 6 '0' $ (showIntAtBase 16 (\d -> hexChars !! d) $ foldr1 (+) $ map (\(x, i) -> x * (2 ^ (8 * i))) $ zip (reverse c) [0..]) ""

grayscalesCountStep :: Int -> Int
grayscalesCountStep count = floor $ 255.0 / (fromIntegral (count - 1))

grayscalesCount :: Int -> [String]
grayscalesCount count = map (\x -> toHexColor [x, x, x]) $ map (\x -> (grayscalesCountStep count) * x) [0..(count - 1)]

radToDeg = (* 360.0) . (/ (pi * 2.0))
degToRad = (* (pi * 2.0)) . (/ 360.0)

srgbToLinear x = if x <= 0.04045 then x / 12.92 else ((x + 0.055) / 1.055) ** 2.4

commaStrings :: [String] -> String
commaStrings = foldr1 (\x acc -> x ++ ", " ++ acc)

color c = (putStr "(") >> (putStr $ commaStrings $ map (\x -> showFFloat (Just 3) x "") $ map srgbToLinear c) >> putStrLn ")"
