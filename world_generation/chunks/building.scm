(define this-chunk
    (fill-area
        (filled-chunk (tile 'concrete))
        (make-point 2 2)
        (make-point (- size-x 4) (- size-y 4))
        (tile 'wood)))

(fill-area
    this-chunk
    (make-point 6 2)
    (make-point 4 2)
    (tile 'concrete))
