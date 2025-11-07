import Numeric

radToDeg = (* 360.0) . (/ (pi * 2.0))
degToRad = (* (pi * 2.0)) . (/ 360.0)

srgbToLinear x = if x <= 0.04045 then x / 12.92 else ((x + 0.055) / 1.055) ** 2.4

commaStrings :: [String] -> String
commaStrings = foldr1 (\x acc -> x ++ ", " ++ acc)

color c = (putStr "(") >> (putStr $ commaStrings $ map (\x -> showFFloat (Just 3) x "") $ map srgbToLinear c) >> putStrLn ")"
