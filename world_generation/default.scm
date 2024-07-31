(define size-x 16)
(define size-y 16)

(define (filled-chunk tile)
    (make-vector (* size-x size-y) tile))

(define (index-of point)
    (+ (* size-x (point-y point)) (point-x point))) 

(define make-point cons)
(define point-x car)
(define point-y cdr)
(define (point-add a b) (make-point (+ (point-x a) (point-x b)) (+ (point-y a) (point-y b))))
(define (point-sub a b) (point-add a (make-point (* (point-x b) -1) (* (point-y b) -1))))

(define make-area cons)
(define area-start car)
(define area-size cdr)
(define (area-end area) (point-add (area-start area) (point-sub (area-size area) (make-point 1 1))))

(define (area-offset area offset)
    (make-area
        (point-add area-start offset)
        (area-size area)))

(define side-up 0)
(define side-down 1)
(define side-left 2)
(define side-right 3)

; advanced rng, why do i even have this?
(define random-side side-up)

(define (put-tile chunk pos tile)
    (vector-set!
        chunk
        (index-of pos)
        tile)
    chunk)

(define (get-tile chunk pos)
    (vector-ref chunk (index-of pos)))

(define (for-each-tile f area)
    (define (for-vertical pos len)
        (if (not (= len 0))
            (begin
                (f (make-point (point-x pos) (- (+ len (point-y pos)) 1)))
                (for-vertical pos (- len 1)))))
    (define pos (area-start area))
    (define size (area-size area))
    (if (not (= (point-x size) 0))
        (begin
            (for-vertical
                (make-point (- (+ (point-x pos) (point-x size)) 1) (point-y pos))
                (point-y size))
            (for-each-tile
                f
                (make-area pos (make-point (- (point-x size) 1) (point-y size)))))))

(define (vertical-line-length chunk pos len tile)
    (for-each-tile
        (lambda (pos) (put-tile chunk pos tile))
        (make-area
            pos
            (make-point 1 len)))
    chunk)

(define (vertical-line chunk x tile)
    (vertical-line-length chunk (make-point x 0) size-y tile))

(define (horizontal-line-length chunk pos len tile)
    (for-each-tile
        (lambda (pos) (put-tile chunk pos tile))
        (make-area
            pos
            (make-point len 1)))
    chunk)

(define (horizontal-line chunk y tile)
    (horizontal-line-length chunk (make-point 0 y) size-x tile))

(define (fill-area chunk area tile)
    (for-each-tile
        (lambda (pos) (put-tile chunk pos tile))
        area)
    chunk)

(define (copy-area chunk area offset)
    (for-each-tile
        (lambda (pos) (put-tile chunk (point-add pos offset) (get-tile chunk pos)))
        area)
    chunk)

; if the destination overlaps the area it will get cut off
(define (move-area chunk area offset)
    (copy-area chunk area offset)
    (fill-area
        chunk
        area
        (tile 'air)))

(define (rectangle-outline-different chunk area up right left down)
    (define pos (area-start area))
    (define size (area-size area))
    (vertical-line-length
        chunk
        pos
        (point-y size)
        left)

    (vertical-line-length
        chunk
        (make-point (- (+ (point-x pos) (point-x size)) 1) (point-y pos))
        (point-y size)
        right)

    (horizontal-line-length
        chunk
        pos
        (point-x size)
        up)

    (horizontal-line-length
        chunk
        (make-point (point-x pos) (- (+ (point-y pos) (point-y size)) 1))
        (point-x size)
        down))

(define (rectangle-outline chunk area tile)
    (rectangle-outline-different chunk area tile tile tile tile))
