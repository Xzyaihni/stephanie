(let ((wall-material (tile (quote concrete))))
    (horizontal-line
        (horizontal-line
            (vertical-line
                (vertical-line
                    (filled-chunk (tile (quote air)))
                    0
                    wall-material)
                (- size-x 1)
                wall-material)
            0
            wall-material)
        (- size-y 1)
        wall-material))
