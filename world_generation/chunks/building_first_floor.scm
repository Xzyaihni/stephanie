(define this-chunk (residential-building))

(define (this-tile point tile) (put-tile this-chunk point tile))

; entrance
(this-tile
    (make-point 7 1)
    (tile 'air))

(this-tile
    (make-point 8 1)
    (tile 'air))