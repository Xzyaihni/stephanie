(define this-chunk
    (fill-area
        (filled-chunk (tile 'air))
        (make-point 1 1)
        (make-point (- size-x 2) (- size-y 2))
        (tile 'concrete)))

(put-tile
    this-chunk
    (make-point 7 2)
    (tile 'stairs_down)))
