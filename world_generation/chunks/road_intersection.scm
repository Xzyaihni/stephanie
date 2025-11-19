(fill-area
    (fill-area
        (filled-chunk (tile 'concrete-path))
        (make-area
            (make-point 2 0)
            (make-point (- size-x 4) size-y))
        (tile 'asphalt))
    (make-area
        (make-point 0 2)
        (make-point size-x (- size-y 4)))
    (tile 'asphalt))
