(define size-x 16)
(define size-y 16)

(define (filled-chunk this-tile)
    (make-vector (* size-x size-y) this-tile))
