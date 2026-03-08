(define path-tile (tile 'concrete-path))

(put-tile
    (put-tile
        (vertical-line
            (filled-chunk (tile 'asphalt))
            0
            path-tile)
        (make-point (- size-x 1) 0)
        path-tile)
    (make-point (- size-x 1) (- size-y 1))
    path-tile)
