(define this-chunk (filled-chunk (tile 'soil)))

(fill-area
    this-chunk
    (make-point 0 0)
    (make-point 4 4)
    (tile 'grassie))

(fill-area
    this-chunk
    (make-point 8 0)
    (make-point 2 1)
    (tile 'glass))

(fill-area
    this-chunk
    (make-point 10 0)
    (make-point 6 2)
    (tile 'concrete))

(fill-area
    this-chunk
    (make-point 7 6)
    (make-point 4 8)
    (tile 'glass))

(fill-area
    this-chunk
    (make-point 3 9)
    (make-point 9 4)
    (tile 'concrete))
