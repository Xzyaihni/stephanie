(define path-tile (tile 'concrete-path))

(put-tile
    (put-tile
        (put-tile
            (put-tile
                (filled-chunk (tile 'asphalt))
                (make-point 0 0)
                path-tile)
            (make-point (- size-x 1) 0)
            path-tile)
        (make-point 0 (- size-y 1))
        path-tile)
    (make-point (- size-x 1) (- size-y 1))
    path-tile)
