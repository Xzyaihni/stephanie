(let ((wall-material (tile (quote concrete))))
    (fill-area (fill-area (horizontal-line
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
        wall-material) (make-point 6 6) (make-point 2 2) (tile (quote soil))) (make-point 10 6) (make-point 1 1) (tile (quote soil))))
