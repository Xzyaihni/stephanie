(fill-area
    (fill-area
        (filled-chunk (tile 'concrete))
        (make-area
            (make-point 2 0)
            (make-point (- size-x 4) size-y))
        (tile 'asphalt))
    (make-area
        (make-point 2 2)
        (make-point (- size-x 2) (- size-y 4)))
    (tile 'asphalt))
