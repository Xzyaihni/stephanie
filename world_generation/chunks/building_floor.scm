(define this-chunk
    (fill-area
        (filled-chunk (tile 'air))
        (make-point 1 1)
        (make-point (- size-x 2) (- size-y 2))
        (tile 'wood)))

(fill-area
    this-chunk
    (make-point 5 0)
    (make-point 6 1)
    (tile 'concrete))

(let ((x (if (= (remainder height 4) 0) 6 9)))
    (put-tile
        this-chunk
        (make-point x 1)
        (tile 'stairs_down)))

