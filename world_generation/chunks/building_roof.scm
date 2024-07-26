(define this-chunk
    (fill-area
        (filled-chunk (tile 'air))
        (make-point 1 1)
        (make-point (- size-x 2) (- size-y 2))
        (tile 'concrete)))

(fill-area
    this-chunk
    (make-point 6 0)
    (make-point 4 1)
    (tile 'concrete))

(put-tile
    this-chunk
    (make-point 6 1)
    (tile 'stairs_down)))
