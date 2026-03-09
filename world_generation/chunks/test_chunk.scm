(define this-chunk (filled-chunk (tile 'soil)))

(fill-area
    this-chunk
    (make-area
        (make-point 0 0)
        (make-point 4 4))
    (tile 'grassie))

(fill-area
    this-chunk
    (make-area
        (make-point 4 4)
        (make-point 2 3))
    (tile 'concrete))

(fill-area
    this-chunk
    (make-area
        (make-point 3 5)
        (make-point 4 1))
    (tile 'glass))
