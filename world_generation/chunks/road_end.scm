(define path-tile (tile 'concrete-path))

(fill-area
    (vertical-line
        (horizontal-line
            (horizontal-line
                (filled-chunk (tile 'asphalt))
                0
                path-tile)
            (- size-y 1)
            path-tile)
        (- size-x 1)
        path-tile)
    (make-area
        (make-point 0 (- (/ size-y 2) 1))
        (make-point 3 2))
    (tile 'asphalt-line rotation))
