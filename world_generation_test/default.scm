(define size-x 8)
(define size-y 8)

(define (filled-chunk this-tile)
    (make-vector (* size-x size-y) this-tile))
